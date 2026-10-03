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
        // Older SDKs lack Sendable annotations on both settings and callback.
        // Keep the callback nonisolated and send only the extracted status.
        await withCheckedContinuation { continuation in
            UNUserNotificationCenter.current().getNotificationSettings { @Sendable settings in
                continuation.resume(returning: settings.authorizationStatus)
            }
        }
    }
    func requestPermission() async throws -> Bool {
        try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .badge, .sound])
    }
    func register() { UIApplication.shared.registerForRemoteNotifications() }
}

extension MobileAuthenticationContext {
    /// Stable across refresh/restart, distinct for every paired login, including
    /// replacement logins for the same account. Never send this credential to Relay.
    var pushScope: String {
        let identity = [serverUrl, userId, installationId, revocationToken ?? refreshToken ?? generation.uuidString]
        return SHA256.hash(data: Data(identity.joined(separator: "|").utf8))
            .map { String(format: "%02x", $0) }.joined()
    }
}

@MainActor
protocol PushRelayBoundary {
    var supported: Bool { get }
    func register(token: String, relayUrl: String, scope: String, validate: @MainActor () throws -> Void) async throws -> RelayGrant
    func revoke(_ grant: RelayGrant, relayUrl: String) async throws
}

/// Thin native/network/storage seams; the coordinator and retry policy remain real.
@MainActor
protocol AppAttestBoundary {
    var supported: Bool { get }
    func generateKey() async throws -> String
    func attestKey(_ key: String, hash: Data) async throws -> Data
    func assertion(_ key: String, hash: Data) async throws -> Data
}

@MainActor
struct NativeAppAttest: AppAttestBoundary {
    var supported: Bool { DCAppAttestService.shared.isSupported }
    func generateKey() async throws -> String { try await DCAppAttestService.shared.generateKey() }
    func attestKey(_ key: String, hash: Data) async throws -> Data {
        try await DCAppAttestService.shared.attestKey(key, clientDataHash: hash)
    }
    func assertion(_ key: String, hash: Data) async throws -> Data {
        try await DCAppAttestService.shared.generateAssertion(key, clientDataHash: hash)
    }
}

@MainActor
protocol PushSetupStorage {
    func load(_ key: String) -> Data?
    func save(_ data: Data, key: String) throws
    func delete(_ key: String)
}

@MainActor
struct KeychainPushSetupStorage: PushSetupStorage {
    func load(_ key: String) -> Data? { key == PushContentKey.storageKey ? SharedPushKeychain.load() : KeychainService.load(for: key) }
    func save(_ data: Data, key: String) throws {
        if key == PushContentKey.storageKey { try SharedPushKeychain.save(data) } else { try KeychainService.save(data, for: key) }
    }
    func delete(_ key: String) {
        if key == PushContentKey.storageKey { SharedPushKeychain.delete() } else { KeychainService.delete(for: key) }
    }
}

@MainActor
protocol PushRelayTransport {
    func send(_ path: String, body: Data, relayUrl: String) async throws -> Data
}

@MainActor
struct NativePushRelayTransport: PushRelayTransport {
    func send(_ path: String, body: Data, relayUrl: String) async throws -> Data {
        guard let base = URL(string: relayUrl), base.scheme == "https",
              let url = URL(string: relayUrl.trimmingCharacters(in: CharacterSet(charactersIn: "/")) + path) else {
            throw PushSetupError.unavailable
        }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 15
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = body
        let session = URLSession(configuration: .ephemeral, delegate: RelayNoRedirects(), delegateQueue: nil)
        defer { session.invalidateAndCancel() }
        let (data, response) = try await session.data(for: request)
        guard let response = response as? HTTPURLResponse, (200...299).contains(response.statusCode) else {
            throw APIError.httpError(statusCode: (response as? HTTPURLResponse)?.statusCode ?? -1, data: Data())
        }
        return data
    }
}

private struct RelayChallenge: Codable {
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

@MainActor
final class AppAttestPushRelay: PushRelayBoundary {
    var supported: Bool { service.supported }
    private let service: any AppAttestBoundary
    private let transport: any PushRelayTransport
    private let storage: any PushSetupStorage
    private let environment: String

    init(
        service: any AppAttestBoundary = NativeAppAttest(),
        transport: any PushRelayTransport = NativePushRelayTransport(),
        storage: any PushSetupStorage = KeychainPushSetupStorage(), environment: String? = nil
    ) {
        self.service = service
        self.transport = transport
        self.storage = storage
        self.environment = environment
            ?? Bundle.main.object(forInfoDictionaryKey: "ServerBeeAPNSEnvironment") as? String ?? ""
    }

    func register(
        token: String, relayUrl: String, scope: String, validate: @MainActor () throws -> Void
    ) async throws -> RelayGrant {
        guard supported, ["sandbox", "production"].contains(environment) else { throw PushSetupError.unavailable }
        try validate()
        let keyStorage = storageKey(relayUrl, scope: scope)
        var keyId = storage.load(keyStorage).flatMap { String(data: $0, encoding: .utf8) }
        if keyId == nil {
            keyId = try await service.generateKey()
            try validate()
            if let keyId { try storage.save(Data(keyId.utf8), key: keyStorage) }
        }
        guard let keyId else { throw PushSetupError.unavailable }
        let isNew = storage.load(keyStorage + "_attested") == nil
        let action = isNew ? "attest" : "renew"
        let challenge = try await registrationChallenge(
            isNew: isNew, keyStorage: keyStorage, request: RelayChallengeRequest(
                action: action, keyId: keyId, deviceToken: token, environment: environment, grantId: nil
            ), relayUrl: relayUrl
        )
        try validate()
        guard let clientData = Data(base64Encoded: challenge.clientData) else { throw PushSetupError.unavailable }
        let digest = Data(SHA256.hash(data: clientData))
        do {
            let proof: Data
            if isNew {
                do {
                    proof = try await service.attestKey(keyId, hash: digest)
                    // Persist native success before HTTP, so a lost admission response
                    // retries with an assertion instead of attesting this key twice.
                    try storage.save(Data([1]), key: keyStorage + "_attested")
                    storage.delete(keyStorage + "_challenge")
                } catch {
                    if !isTransientAttestationError(error) { discardKey(keyStorage) }
                    throw error
                }
            } else {
                do { proof = try await service.assertion(keyId, hash: digest) } catch {
                    if isInvalidKey(error) { discardKey(keyStorage) }
                    throw error
                }
            }
            try validate()
            let grant: RelayGrant = try await send(
                "/v1/\(action)", body: RelayProof(challengeId: challenge.challengeId, proof: proof.base64EncodedString()),
                relayUrl: relayUrl
            )
            guard grant.deviceToken == token, grant.environment == environment, grant.keyId == keyId else {
                throw PushSetupError.unavailable
            }
            return grant
        } catch {
            if case APIError.httpError(let status, _) = error, status == 403 { discardKey(keyStorage) }
            throw error
        }
    }

    func revoke(_ grant: RelayGrant, relayUrl: String) async throws {
        guard supported else { throw PushSetupError.unavailable }
        let challenge: RelayChallenge = try await send(
            "/v1/challenges", body: RelayChallengeRequest(
                action: "revoke", keyId: grant.keyId, deviceToken: grant.deviceToken,
                environment: grant.environment, grantId: grant.grantId
            ), relayUrl: relayUrl
        )
        guard let data = Data(base64Encoded: challenge.clientData) else { throw PushSetupError.unavailable }
        let proof = try await service.assertion(grant.keyId, hash: Data(SHA256.hash(data: data)))
        let _: RelayRevocation = try await send(
            "/v1/revoke", body: RelayProof(challengeId: challenge.challengeId, proof: proof.base64EncodedString()), relayUrl: relayUrl
        )
    }

    private func registrationChallenge(
        isNew: Bool, keyStorage: String, request: RelayChallengeRequest, relayUrl: String
    ) async throws -> RelayChallenge {
        if isNew, let data = storage.load(keyStorage + "_challenge"),
           let saved = try? JSONDecoder.snakeCase.decode(SavedAttestationChallenge.self, from: data),
           saved.deadline > Date().timeIntervalSince1970, saved.deviceToken == request.deviceToken {
            return saved.challenge
        }
        let challenge: RelayChallenge = try await send("/v1/challenges", body: request, relayUrl: relayUrl)
        if isNew {
            // Apple serverUnavailable retries retain the same native key/hash.
            // Allow a safety margin inside Relay's five-minute challenge lifetime.
            let saved = SavedAttestationChallenge(
                challenge: challenge, deviceToken: request.deviceToken, deadline: Date().timeIntervalSince1970 + 240
            )
            try storage.save(JSONEncoder.snakeCase.encode(saved), key: keyStorage + "_challenge")
        }
        return challenge
    }

    private func storageKey(_ url: String, scope: String) -> String {
        let digest = SHA256.hash(data: Data("\(url)|\(environment)|\(scope)".utf8))
        return "serverbee_app_attest_" + digest.map { String(format: "%02x", $0) }.joined()
    }

    private func discardKey(_ key: String) {
        storage.delete(key)
        storage.delete(key + "_attested")
        storage.delete(key + "_challenge")
    }

    private func isInvalidKey(_ error: Error) -> Bool {
        let value = error as NSError
        return value.domain == DCError.errorDomain && value.code == DCError.Code.invalidKey.rawValue
    }

    private func isTransientAttestationError(_ error: Error) -> Bool {
        let value = error as NSError
        return (value.domain == DCError.errorDomain && value.code == DCError.Code.serverUnavailable.rawValue)
            || value.domain == NSURLErrorDomain
    }

    private func send<T: Decodable, Body: Encodable>(_ path: String, body: Body, relayUrl: String) async throws -> T {
        let data = try await transport.send(path, body: JSONEncoder.snakeCase.encode(body), relayUrl: relayUrl)
        return try JSONDecoder.snakeCase.decode(T.self, from: data)
    }
}

private struct SavedAttestationChallenge: Codable {
    let challenge: RelayChallenge
    let deviceToken: String
    let deadline: TimeInterval
    enum CodingKeys: String, CodingKey {
        case challenge, deadline
        case deviceToken = "device_token"
    }
}

private struct RelayRevocation: Decodable {
    let revoked: Bool
    enum CodingKeys: String, CodingKey { case revoked }
}

enum PushSetupError: Error, LocalizedError {
    case unavailable
    case insecureServer
    var errorDescription: String? {
        switch self {
        case .unavailable: String(localized: "Verified push is unavailable. Monitoring and login still work.")
        case .insecureServer: String(localized: "Encrypted notification setup requires an HTTPS Server.")
        }
    }
}

private final class RelayNoRedirects: NSObject, URLSessionTaskDelegate, @unchecked Sendable {
    func urlSession(
        _ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
        newRequest request: URLRequest, completionHandler: @escaping @Sendable (URLRequest?) -> Void
    ) {
        completionHandler(nil)
    }
}
