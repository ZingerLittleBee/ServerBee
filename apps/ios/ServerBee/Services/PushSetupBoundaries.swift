import CryptoKit
@preconcurrency import DeviceCheck
import Foundation
import UIKit
import UserNotifications

@MainActor
protocol PushSystemBoundary {
    func authorization() async -> UNAuthorizationStatus
    func requestPermission() async throws -> Bool
    func register()
}

@MainActor
struct NativePushSystem: PushSystemBoundary {
    func authorization() async -> UNAuthorizationStatus {
        await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
    }
    func requestPermission() async throws -> Bool {
        try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .badge, .sound])
    }
    func register() { UIApplication.shared.registerForRemoteNotifications() }
}

@MainActor
protocol PushRelayBoundary {
    var supported: Bool { get }
    func register(token: String, relayUrl: String) async throws -> RelayGrant
    func revoke(_ grant: RelayGrant, relayUrl: String) async throws
}

private struct RelayChallenge: Decodable {
    let challengeId: String
    let clientData: String
    enum CodingKeys: String, CodingKey {
        case challengeId = "challenge_id"
        case clientData = "client_data"
    }
}

private struct RelayChallengeRequest: Encodable {
    let action: String
    let keyId: String
    let deviceToken: String
    let environment: String
    let grantId: String?
    enum CodingKeys: String, CodingKey {
        case action, environment
        case grantId = "grant_id"
        case keyId = "key_id"
        case deviceToken = "device_token"
    }
}

private struct RelayProof: Encodable {
    let challengeId: String
    let proof: String
    enum CodingKeys: String, CodingKey {
        case challengeId = "challenge_id"
        case proof
    }
}

/// App Attest is the only admission path, including on development builds.
@MainActor
final class AppAttestPushRelay: PushRelayBoundary {
    var supported: Bool { DCAppAttestService.shared.isSupported }
    private let service = DCAppAttestService.shared
    private let environment: String

    init() {
        // This value and aps-environment are driven by the same build setting.
        environment = Bundle.main.object(forInfoDictionaryKey: "ServerBeeAPNSEnvironment") as? String ?? ""
    }

    func register(token: String, relayUrl: String) async throws -> RelayGrant {
        guard supported, ["sandbox", "production"].contains(environment) else { throw PushSetupError.unavailable }
        let storage = storageKey(relayUrl)
        var keyId = KeychainService.loadString(for: storage)
        let isNew = keyId == nil || KeychainService.loadString(for: storage + "_attested") != "yes"
        if keyId == nil {
            keyId = try await service.generateKey()
            if let keyId { try KeychainService.saveString(keyId, for: storage) }
        }
        guard let keyId else { throw PushSetupError.unavailable }
        let action = isNew ? "attest" : "renew"
        let challenge: RelayChallenge = try await send(
            "/v1/challenges", body: RelayChallengeRequest(action: action, keyId: keyId, deviceToken: token, environment: environment, grantId: nil), relayUrl: relayUrl
        )
        guard let clientData = Data(base64Encoded: challenge.clientData) else { throw PushSetupError.unavailable }
        let digest = Data(SHA256.hash(data: clientData))
        do {
            let proof: Data
            if isNew {
                do {
                    proof = try await service.attestKey(keyId, clientDataHash: digest)
                    try KeychainService.saveString("yes", for: storage + "_attested")
                } catch {
                    KeychainService.delete(for: storage)
                    throw error
                }
            } else {
                proof = try await service.generateAssertion(keyId, clientDataHash: digest)
            }
            let grant: RelayGrant = try await send(
                "/v1/\(action)", body: RelayProof(challengeId: challenge.challengeId, proof: proof.base64EncodedString()), relayUrl: relayUrl
            )
            guard grant.deviceToken == token, grant.environment == environment, grant.keyId == keyId else { throw PushSetupError.unavailable }
            return grant
        } catch {
            // A rejected new attestation must be retryable with a fresh key. A
            // lost response after admission can still renew the retained key.
            if case APIError.httpError(let status, _) = error, status == 403 {
                KeychainService.delete(for: storage)
                KeychainService.delete(for: storage + "_attested")
            }
            throw error
        }
    }

    func revoke(_ grant: RelayGrant, relayUrl: String) async throws {
        guard supported else { throw PushSetupError.unavailable }
        let challenge: RelayChallenge = try await send(
            "/v1/challenges", body: RelayChallengeRequest(action: "revoke", keyId: grant.keyId, deviceToken: grant.deviceToken, environment: grant.environment, grantId: grant.grantId), relayUrl: relayUrl
        )
        guard let data = Data(base64Encoded: challenge.clientData) else { throw PushSetupError.unavailable }
        let proof = try await service.generateAssertion(grant.keyId, clientDataHash: Data(SHA256.hash(data: data)))
        let _: RelayRevocation = try await send(
            "/v1/revoke", body: RelayProof(challengeId: challenge.challengeId, proof: proof.base64EncodedString()), relayUrl: relayUrl
        )
    }

    private func storageKey(_ url: String) -> String {
        "serverbee_app_attest_" + SHA256.hash(data: Data("\(url)|\(environment)".utf8)).map { String(format: "%02x", $0) }.joined()
    }

    private func send<T: Decodable, Body: Encodable>(_ path: String, body: Body, relayUrl: String) async throws -> T {
        guard let base = URL(string: relayUrl), base.scheme == "https", let url = URL(string: relayUrl.trimmingCharacters(in: CharacterSet(charactersIn: "/")) + path) else {
            throw PushSetupError.unavailable
        }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 15
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder.snakeCase.encode(body)
        // A separate session carries no Server bearer credentials or cookies.
        let session = URLSession(configuration: .ephemeral, delegate: RelayNoRedirects(), delegateQueue: nil)
        defer { session.invalidateAndCancel() }
        let (data, response) = try await session.data(for: request)
        guard let response = response as? HTTPURLResponse, (200...299).contains(response.statusCode) else {
            throw APIError.httpError(statusCode: (response as? HTTPURLResponse)?.statusCode ?? -1, data: Data())
        }
        return try JSONDecoder.snakeCase.decode(T.self, from: data)
    }
}

private struct RelayRevocation: Decodable {
    let revoked: Bool
    enum CodingKeys: String, CodingKey { case revoked }
}

enum PushSetupError: Error, LocalizedError {
    case unavailable
    var errorDescription: String? { String(localized: "Verified push is unavailable. Monitoring and login still work.") }
}


private final class RelayNoRedirects: NSObject, URLSessionTaskDelegate, @unchecked Sendable {
    func urlSession(
        _ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
        newRequest request: URLRequest, completionHandler: @escaping @Sendable (URLRequest?) -> Void
    ) {
        completionHandler(nil)
    }
}
