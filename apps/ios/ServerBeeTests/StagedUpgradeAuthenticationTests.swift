import Foundation
import XCTest
@testable import ServerBee

/// Models the old app only at its HTTP/Keychain boundary; all upgraded session
/// restoration, refresh, registration and cleanup use the production client.
@MainActor
final class StagedUpgradeAuthenticationTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(AuthenticationURLProtocol.self) }

    override func tearDown() async throws {
        URLProtocol.unregisterClass(AuthenticationURLProtocol.self)
        AuthenticationURLProtocol.handler = nil
        AuthenticationURLProtocol.cancelPending()
        AuthManager().clearAuth()
        try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
    }

    nonisolated private static func tokens(_ rotation: Int, loginProof: Bool = false) -> Data {
        let proof = loginProof ? #", "revocation_token":"unknown-server-login-proof""# : ""
        return Data("""
        {"data":{"access_token":"access-\(rotation)","access_expires_in_secs":900,
        "refresh_token":"refresh-\(rotation)","refresh_expires_in_secs":3600,"token_type":"Bearer",
        "user":{"id":"alice","username":"alice","role":"member"}\(proof)}}
        """.utf8)
    }

    private func restoreOldClient(newServerLogin: Bool, rotations: Int) async throws {
        AuthManager().clearAuth()
        try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
        let log = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in
            let count = log.append(request.request)
            switch request.request.url?.path {
            case "/api/mobile/auth/login": request.respond(200, data: Self.tokens(0, loginProof: newServerLogin))
            case "/api/mobile/auth/refresh":
                XCTAssertEqual(Self.body(request.request)["refresh_token"] as? String, "refresh-\(count - 1)")
                request.respond(200, data: Self.tokens(count))
            default: XCTFail("Unexpected old-client request"); request.respond(400)
            }
        }
        try KeychainService.saveString("https://staged-upgrade.test", for: KeychainService.serverUrlKey)
        let installationId = InstallationID.getOrCreate()
        var pair = try await oldClientRequest("/api/mobile/auth/login", body: MobileLoginRequest(
            username: "alice", password: "fixture-password", installationId: installationId, deviceName: "Old app"
        ))
        for _ in 0..<rotations {
            pair = try await oldClientRequest("/api/mobile/auth/refresh", body: MobileRefreshRequest(
                refreshToken: pair.refreshToken, installationId: installationId
            ))
        }
        // The old app discarded every consumed secret and the unknown proof.
        // Only its final pair and user survive the subsequent iOS upgrade.
        try KeychainService.saveString(pair.accessToken, for: KeychainService.accessTokenKey)
        try KeychainService.saveString(pair.refreshToken, for: KeychainService.refreshTokenKey)
        try KeychainService.saveCodable(pair.user, for: KeychainService.userKey)
        XCTAssertNil(KeychainService.loadString(for: KeychainService.revocationTokenKey))
        XCTAssertEqual(pair.refreshToken, "refresh-\(rotations)")
    }

    private func oldClientRequest(_ path: String, body: some Encodable) async throws -> MobileTokenResponse {
        var request = URLRequest(url: try XCTUnwrap(URL(string: "https://staged-upgrade.test\(path)")))
        request.httpMethod = "POST"
        request.httpBody = try JSONEncoder.snakeCase.encode(body)
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        let (data, response) = try await URLSession.shared.data(for: request)
        XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 200)
        return try JSONDecoder.snakeCase.decode(ApiResponse<MobileTokenResponse>.self, from: data).data
    }

    func testServerUpgradeThenOldRotationsRestoreWithLostResponse() async throws {
        for rotations in [1, 3] { try await assertRestoreLoss(newServerLogin: false, rotations: rotations) }
    }

    func testOldLoginOnNewServerThenIOSUpgradeRestoresWithLostResponse() async throws {
        for rotations in [0, 1, 3] { try await assertRestoreLoss(newServerLogin: true, rotations: rotations) }
    }

    private func assertRestoreLoss(newServerLogin: Bool, rotations: Int) async throws {
        try await restoreOldClient(newServerLogin: newServerLogin, rotations: rotations)
        let log = AuthenticationRequestLog()
        let proof = "refresh-\(rotations)"
        let installationId = InstallationID.getOrCreate()
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            switch request.request.url?.path {
            case "/api/mobile/auth/refresh":
                XCTAssertEqual(Self.body(request.request)["refresh_token"] as? String, proof)
                XCTAssertEqual((try? AuthManager.readAuthentication())?.revocationToken, proof,
                               "The legacy fallback stays only in the normal-auth snapshot before refresh")
                for key in [KeychainService.accessTokenKey, KeychainService.refreshTokenKey,
                            KeychainService.revocationTokenKey, KeychainService.userKey] {
                    XCTAssertNil(KeychainService.load(for: key))
                }
                XCTAssertNil(KeychainService.load(for: PrivateSessionRevocationStorage.key))
                request.loseResponse()
            case "/api/mobile/auth/revoke":
                XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
                XCTAssertEqual(Self.body(request.request)["revocation_token"] as? String, proof)
                XCTAssertEqual(Self.body(request.request)["installation_id"] as? String, installationId)
                XCTAssertEqual((try? AuthManager.readAuthentication())?.revocationToken, proof,
                               "Keep the original proof until cleanup completes")
                request.respond(200)
            default: XCTFail("Reachable restoration cleanup must revoke the original login"); request.respond(401)
            }
        }
        let restored = AuthManager()
        await restored.initialize()
        XCTAssertFalse(restored.isAuthenticated)
        XCTAssertFalse(restored.isLoading)
        XCTAssertNil(restored.getAccessToken())
        XCTAssertNil(try AuthManager.readAuthentication())
        XCTAssertEqual(log.snapshot().compactMap { $0.url?.path }, ["/api/mobile/auth/refresh", "/api/mobile/auth/revoke"])
        XCTAssertTrue(log.snapshot().allSatisfy { $0.url?.host == "staged-upgrade.test" })
    }

    func testUpgradedOrdinaryRefreshPreservesRegistrationAndSavedProof() async throws {
        try await restoreOldClient(newServerLogin: true, rotations: 3)
        let log = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in
            let count = log.append(request.request)
            if request.request.url?.path == "/api/mobile/auth/refresh" {
                request.respond(200, data: Self.tokens(3 + count))
            } else {
                XCTAssertEqual(request.request.url?.path, "/api/mobile/push/register")
                XCTAssertEqual(request.request.value(forHTTPHeaderField: "Authorization"), "Bearer access-6")
                request.respond(200)
            }
        }
        let restored = AuthManager()
        await restored.initialize()
        let context = try XCTUnwrap(restored.captureContext())
        for _ in 0..<2 {
            _ = try await restored.refreshAccessToken(context: context)
            XCTAssertTrue(restored.isCurrent(context))
            XCTAssertEqual((try? AuthManager.readAuthentication())?.revocationToken, "refresh-3")
            XCTAssertNil((try? AuthManager.readAuthentication())?.confirmedDeletionProof)
            XCTAssertTrue(try PendingSessionRevocations().records().isEmpty)
        }
        try await APIClient(authManager: restored).postVoid(
            "/api/mobile/push/register", body: ["device_token": "preserved-registration"], context: context
        )
        XCTAssertEqual(log.snapshot().filter { $0.url?.path == "/api/mobile/push/register" }.count, 1)
        XCTAssertTrue(restored.isAuthenticated)
    }

    func testStagedStartupCleanupPreservesReplacementLogin() async throws {
        try await restoreOldClient(newServerLogin: false, rotations: 3)
        let log = AuthenticationRequestLog()
        let cleanup = expectation(description: "startup cleanup held before its response")
        cleanup.assertForOverFulfill = false
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            if request.request.url?.path == "/api/mobile/auth/refresh" { request.loseResponse() } else {
                XCTAssertEqual(request.request.url?.path, "/api/mobile/auth/revoke")
                XCTAssertEqual(Self.body(request.request)["revocation_token"] as? String, "refresh-3")
                log.hold(request)
                cleanup.fulfill()
            }
        }
        let restored = AuthManager()
        let startup = Task { await restored.initialize() }
        await fulfillment(of: [cleanup], timeout: 3)
        restored.clearAuth()
        restored.setServerUrl("https://replacement.test")
        restored.handleLoginResponse(MobileTokenResponse(
            accessToken: "replacement-access", accessExpiresInSecs: 900,
            refreshToken: "replacement-refresh", refreshExpiresInSecs: 3600,
            tokenType: "Bearer", user: MobileUser(id: "bob", username: "bob", role: "member"),
            revocationToken: "replacement-proof", mobileSessionId: "22222222-2222-4222-8222-222222222222"
        ))
        try XCTUnwrap(log.takeHeld()).respond(200)
        await startup.value
        XCTAssertTrue(restored.isAuthenticated)
        XCTAssertEqual(restored.user?.id, "bob")
        XCTAssertEqual(restored.getAccessToken(), "replacement-access")
        XCTAssertEqual((try? AuthManager.readAuthentication())?.revocationToken, "replacement-proof")
        XCTAssertTrue(log.snapshot().allSatisfy { $0.url?.host == "staged-upgrade.test" })
        XCTAssertEqual(log.snapshot().filter { $0.url?.path == "/api/mobile/auth/revoke" }.count, 1)
    }

    func testCurrentSecretRevokesBeforeOverlappingRefreshWithoutBearerFallback() async throws {
        try await restoreOldClient(newServerLogin: true, rotations: 1)
        let auth = AuthManager()
        // This overlap scenario starts from the last old-client pair. Separate
        // restoration tests exercise migration of the same legacy Keychain inputs.
        auth.clearAuth()
        auth.setServerUrl("https://staged-upgrade.test")
        auth.handleLoginResponse(try JSONDecoder.snakeCase.decode(ApiResponse<MobileTokenResponse>.self, from: Self.tokens(1)).data)
        let context = try XCTUnwrap(auth.captureContext())
        let log = AuthenticationRequestLog()
        let started = expectation(description: "HTTP refresh not yet consumed by Server")
        started.assertForOverFulfill = false
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            if request.request.url?.path == "/api/mobile/auth/refresh" {
                log.hold(request)
                started.fulfill()
            } else {
                XCTAssertEqual(request.request.url?.path, "/api/mobile/auth/revoke")
                XCTAssertEqual(Self.body(request.request)["revocation_token"] as? String, "refresh-1")
                XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
                request.respond(200)
            }
        }
        let refresh = Task { try await auth.refreshAccessToken(context: context) }
        await fulfillment(of: [started], timeout: 3)
        await auth.endSession(context: context)
        XCTAssertFalse(auth.isAuthenticated)
        try XCTUnwrap(log.takeHeld()).respond(401)
        do {
            _ = try await refresh.value
            XCTFail("A late refresh cannot restore the revoked login")
        } catch AuthError.staleIdentity { /* Expected original generation rejection. */ }
        XCTAssertFalse(auth.isAuthenticated)
        XCTAssertNil(auth.getAccessToken())
        XCTAssertNil(try AuthManager.readAuthentication())
        XCTAssertEqual(log.snapshot().compactMap { $0.url?.path }, ["/api/mobile/auth/refresh", "/api/mobile/auth/revoke"])
    }

    nonisolated private static func body(_ request: URLRequest) -> [String: Any] {
        var data = request.httpBody ?? Data()
        if data.isEmpty, let stream = request.httpBodyStream {
            stream.open()
            defer { stream.close() }
            var buffer = [UInt8](repeating: 0, count: 1024)
            while stream.hasBytesAvailable {
                let count = stream.read(&buffer, maxLength: buffer.count)
                if count <= 0 { break }
                data.append(contentsOf: buffer.prefix(count))
            }
        }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
    }
}
