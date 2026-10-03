import Foundation
import Security
import XCTest
@testable import ServerBee

/// App-hosted Security.framework queries establish storage behavior on the
/// selected Simulator. They do not prove a signed physical extension's rights.
@MainActor
final class PushKeychainIsolationTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(AuthenticationURLProtocol.self) }

    override func tearDown() async throws {
        AuthenticationURLProtocol.handler = nil
        AuthenticationURLProtocol.cancelPending()
        URLProtocol.unregisterClass(AuthenticationURLProtocol.self)
        KeychainService.delete(for: "fixture-pending-relay-grant")
        AuthManager().clearAuth()
    }

    func testLoginAndRefreshKeepCredentialsPrivateAndOnlyContentKeyShared() async throws {
        let auth = AuthManager()
        auth.setServerUrl("https://keychain-fixture.test")
        auth.handleLoginResponse(tokens(access: "fixture-access", refresh: "fixture-refresh"))
        let context = try XCTUnwrap(auth.captureContext())
        let privateGroup = try XCTUnwrap(Bundle.main.object(forInfoDictionaryKey: "PrivateKeychainAccessGroup") as? String)
        let sharedGroup = try XCTUnwrap(Bundle.main.object(forInfoDictionaryKey: "PushKeychainAccessGroup") as? String)
        XCTAssertNotEqual(privateGroup, sharedGroup)
        let storage = KeychainPushSetupStorage()
        let content = PushContentKey(keyId: UUID().uuidString, key: Data(repeating: 0x41, count: 32).base64EncodedString(),
                                     deploymentId: context.serverUrl, userId: context.userId,
                                     installationId: context.installationId, scope: context.pushScope)
        let bytes = try JSONEncoder().encode(content)
        try storage.save(bytes, key: PushContentKey.storageKey)
        try storage.save(Data("fixture-grant-secret".utf8), key: "fixture-pending-relay-grant")
        XCTAssertEqual(SharedPushKeychain.load(), bytes)
        XCTAssertNil(KeychainService.load(for: PushContentKey.storageKey))
        XCTAssertEqual(try read(group: sharedGroup, service: "com.serverbee.mobile.push", account: PushContentKey.storageKey), bytes)
        XCTAssertNil(try read(group: privateGroup, service: "com.serverbee.mobile.push", account: PushContentKey.storageKey))
        try assertPrivateCredentials(privateGroup: privateGroup, sharedGroup: sharedGroup,
                                     access: "fixture-access", refresh: "fixture-refresh")

        let rotatedToken = try JSONEncoder().encode(tokens(access: "rotated-access", refresh: "rotated-refresh"))
        var rotated = Data(#"{"data":"#.utf8)
        rotated.append(rotatedToken)
        rotated.append(Data("}".utf8))
        let response = rotated
        AuthenticationURLProtocol.handler = { request in request.respond(200, data: response) }
        _ = try await auth.refreshAccessToken(context: context)
        XCTAssertTrue(auth.isCurrent(context))
        XCTAssertEqual(context.pushScope, try XCTUnwrap(auth.captureContext()).pushScope)
        XCTAssertEqual(SharedPushKeychain.load(), bytes)
        try assertPrivateCredentials(privateGroup: privateGroup, sharedGroup: sharedGroup,
                                     access: "rotated-access", refresh: "rotated-refresh")

        auth.clearAuth()
        XCTAssertNil(SharedPushKeychain.load())
        for key in [KeychainService.accessTokenKey, KeychainService.refreshTokenKey, KeychainService.revocationTokenKey] {
            XCTAssertNil(try read(group: privateGroup, service: "com.serverbee.mobile", account: key))
            XCTAssertNil(try read(group: sharedGroup, service: "com.serverbee.mobile", account: key))
        }
    }

    private func tokens(access: String, refresh: String) -> MobileTokenResponse {
        MobileTokenResponse(accessToken: access, accessExpiresInSecs: 900,
                            refreshToken: refresh, refreshExpiresInSecs: 3600, tokenType: "Bearer",
                            user: MobileUser(id: "alice", username: "alice", role: "member"),
                            revocationToken: "fixture-stable-revocation")
    }

    private func assertPrivateCredentials(privateGroup: String, sharedGroup: String, access: String, refresh: String) throws {
        for (key, value) in [(KeychainService.accessTokenKey, access), (KeychainService.refreshTokenKey, refresh),
                             (KeychainService.revocationTokenKey, "fixture-stable-revocation"),
                             ("fixture-pending-relay-grant", "fixture-grant-secret")] {
            XCTAssertEqual(try read(group: privateGroup, service: "com.serverbee.mobile", account: key), Data(value.utf8))
            // Query the complete credential service from the extension's group,
            // rather than only checking the separate content-key service.
            XCTAssertNil(try read(group: sharedGroup, service: "com.serverbee.mobile", account: key))
        }
    }

    private func read(group: String, service: String, account: String) throws -> Data? {
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                   kSecAttrAccessGroup as String: group, kSecAttrService as String: service,
                                   kSecAttrAccount as String: account, kSecReturnData as String: true]
        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        XCTAssertEqual(status, errSecSuccess, "Keychain query failure is not evidence of isolation")
        guard status == errSecSuccess else { throw KeychainError.saveFailed(status) }
        return result as? Data
    }
}
