import Foundation
import XCTest
@testable import ServerBee

private struct UpgradedSessionFixture {
    let auth: AuthManager
    let api: APIClient
    let manager: PushNotificationManager
}

/// The old Server's HTTP boundary replaces its session on every refresh, as
/// pinned baseline 923a255 does. Client persistence and all cleanup are real.
@MainActor
final class IOSFirstUpgradeRevocationTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(AuthenticationURLProtocol.self) }

    override func tearDown() async throws {
        URLProtocol.unregisterClass(AuthenticationURLProtocol.self)
        AuthenticationURLProtocol.handler = nil
        AuthManager().clearAuth()
    }

    func testStartupRestorationRecoversStaleProofAfterCommittedResponseLoss() async throws {
        for replacements in [1, 3] { try await assertLostResponseCleanup(replacements, route: "startup") }
    }

    func testForegroundExpiryRecoversStaleProofAfterCommittedResponseLoss() async throws {
        for replacements in [1, 3] { try await assertLostResponseCleanup(replacements, route: "api") }
    }

    func testLongLivedPushContextRecoversStaleProofAfterCommittedResponseLoss() async throws {
        for replacements in [1, 3] { try await assertLostResponseCleanup(replacements, route: "push") }
    }

    func testExplicitLogoutRecoversStaleProofAfterCommittedResponseLoss() async throws {
        for replacements in [1, 3] { try await assertLostResponseCleanup(replacements, route: "logout") }
    }

    private func assertLostResponseCleanup(_ replacements: Int, route: String) async throws {
        let fixture = try await prepareUpgradedSession(replacements, restore: route == "startup")
        let auth = fixture.auth
        let api = fixture.api
        let manager = fixture.manager
        let log = AuthenticationRequestLog()
        let capturedSecret = "refresh-\(replacements + 1)"
        let installationId = InstallationID.getOrCreate()
        let revoked = expectation(description: "captured current secret recovers the stale proof")
        revoked.assertForOverFulfill = false
        AuthenticationURLProtocol.handler = { request in
            let count = log.append(request.request)
            switch request.request.url?.path {
            case "/api/mobile/auth/refresh":
                XCTAssertEqual(Self.body(request.request)["refresh_token"] as? String, capturedSecret)
                if count == 1 { request.loseResponse() } else { request.respond(401) }
            case "/api/mobile/auth/revoke":
                XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
                XCTAssertEqual(Self.body(request.request)["installation_id"] as? String, installationId)
                let proof = Self.body(request.request)["revocation_token"] as? String
                XCTAssertEqual(proof, count == 1 ? "refresh-0" : capturedSecret)
                XCTAssertEqual(KeychainService.loadString(for: KeychainService.refreshTokenKey), capturedSecret,
                               "Keep the captured original credential until cleanup succeeds")
                request.respond(count == 1 ? 401 : 200)
                if count == 2 { revoked.fulfill() }
            case "/api/servers", "/api/mobile/push/unregister", "/api/mobile/push/register": request.respond(401)
            default: XCTFail("Stale proof recovery must not enter bearer logout or ordinary authentication"); request.respond(401)
            }
        }
        let subject: AuthManager
        if route == "startup" {
            subject = AuthManager()
            await subject.initialize()
        } else {
            subject = auth
            do {
                _ = try await auth.refreshAccessToken()
                XCTFail("Expected committed response loss")
            } catch AuthError.refreshNetworkFailure { /* The transport failure preserves the original login. */ }
            XCTAssertTrue(auth.isAuthenticated)
            XCTAssertEqual(KeychainService.loadString(for: KeychainService.revocationTokenKey), "refresh-0")
            if route == "api" {
                do {
                    let _: String = try await api.get("/api/servers")
                    XCTFail("Expected unauthorized")
                } catch APIError.unauthorized { /* Production automatic cleanup recovers the captured proof. */ }
            } else if route == "push" {
                // The manager has kept its original pre-upgrade context, whose
                // proof and refresh token both predate all session replacements.
                manager.didRegisterForRemoteNotifications(deviceToken: Data([3, 4]))
                await fulfillment(of: [revoked], timeout: 3)
                await manager.unregister() // Drain the actual upload before checking auth cleanup.
            } else {
                await SettingsViewModel().logout(
                    authManager: auth, apiClient: api, pushManager: manager, closeWebSocket: {}
                )
            }
        }
        if route != "push" { await fulfillment(of: [revoked], timeout: 3) }
        XCTAssertFalse(subject.isAuthenticated)
        XCTAssertNil(subject.getAccessToken())
        XCTAssertNil(KeychainService.loadString(for: KeychainService.refreshTokenKey))
        XCTAssertNil(KeychainService.loadString(for: KeychainService.revocationTokenKey))
        let proofs = log.snapshot().filter { $0.url?.path == "/api/mobile/auth/revoke" }
        XCTAssertEqual(proofs.compactMap { Self.body($0)["revocation_token"] as? String }, ["refresh-0", capturedSecret])
        if route != "push" { XCTAssertEqual(log.snapshot().last?.url?.path, "/api/mobile/auth/revoke") } else {
            XCTAssertEqual(log.snapshot().filter { $0.url?.path == "/api/mobile/push/register" }.count, 1)
        }
        XCTAssertTrue(log.snapshot().allSatisfy { $0.url?.host == "ios-first-upgrade.test" })
    }

    func testLogoutBeforeRefreshConsumptionRecoversStaleProof() async throws {
        try await assertLogoutHandoff(committed: false)
    }

    func testLogoutAfterRefreshCommitRecoversStaleProofWithLostResponse() async throws {
        try await assertLogoutHandoff(committed: true)
    }

    private func assertLogoutHandoff(committed: Bool) async throws {
        let fixture = try await prepareUpgradedSession(3, restore: false)
        let auth = fixture.auth
        let api = fixture.api
        let manager = fixture.manager
        let log = AuthenticationRequestLog()
        let started = expectation(description: "overlapping original refresh is held")
        started.assertForOverFulfill = false
        let unregister = expectation(description: "logout reaches unregister before refresh response resolves")
        unregister.assertForOverFulfill = false
        AuthenticationURLProtocol.handler = { request in
            let count = log.append(request.request)
            switch request.request.url?.path {
            case "/api/mobile/auth/refresh":
                XCTAssertEqual(Self.body(request.request)["refresh_token"] as? String, "refresh-4")
                if count == 1 {
                    log.hold(request)
                    started.fulfill()
                } else { request.respond(401) }
            case "/api/mobile/push/unregister":
                request.respond(committed ? 401 : 200)
                unregister.fulfill()
            case "/api/mobile/auth/revoke":
                XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
                XCTAssertEqual(Self.body(request.request)["revocation_token"] as? String, count == 1 ? "refresh-0" : "refresh-4")
                request.respond(count == 1 ? 401 : 200)
            default: XCTFail("Cleanup must stay inside the original proof endpoint"); request.respond(401)
            }
        }
        let refresh = Task { try await auth.refreshAccessToken() }
        await fulfillment(of: [started], timeout: 3)
        let logout = Task {
            await SettingsViewModel().logout(authManager: auth, apiClient: api, pushManager: manager, closeWebSocket: {})
        }
        await fulfillment(of: [unregister], timeout: 3)
        if committed {
            try XCTUnwrap(log.takeHeld()).loseResponse()
            do { _ = try await refresh.value; XCTFail("Expected committed response loss") } catch AuthError.refreshNetworkFailure { /* Logout must still revoke after the lost response. */ }
            await logout.value
        } else {
            await logout.value
            try XCTUnwrap(log.takeHeld()).respond(401)
            do { _ = try await refresh.value; XCTFail("Revoked login must reject late refresh") } catch AuthError.staleIdentity { /* Cleanup already ended that generation. */ }
        }
        XCTAssertFalse(auth.isAuthenticated)
        XCTAssertNil(auth.getAccessToken())
        XCTAssertNil(KeychainService.loadString(for: KeychainService.revocationTokenKey))
        XCTAssertEqual(log.snapshot().filter { $0.url?.path == "/api/mobile/auth/revoke" }.count, 2)
        let refreshCount = log.snapshot().filter { $0.url?.path == "/api/mobile/auth/refresh" }.count
        if committed {
            XCTAssertTrue((1...2).contains(refreshCount), "Cleanup may join the held refresh or retry the same original secret once")
        } else { XCTAssertEqual(refreshCount, 1) }
        XCTAssertTrue(log.snapshot().allSatisfy { $0.url?.host == "ios-first-upgrade.test" })
    }

    func testStaleProofRecoveryNeverUsesReplacementAccountOrDeploymentSecret() async throws {
        let auth = try await prepareUpgradedSession(3, restore: false).auth
        let log = AuthenticationRequestLog()
        let rejected = expectation(description: "stale proof rejection held while login changes")
        rejected.assertForOverFulfill = false
        AuthenticationURLProtocol.handler = { request in
            let count = log.append(request.request)
            XCTAssertEqual(request.request.url?.path, "/api/mobile/auth/revoke")
            XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
            if count == 1 {
                XCTAssertEqual(Self.body(request.request)["revocation_token"] as? String, "refresh-0")
                log.hold(request)
                rejected.fulfill()
            } else {
                XCTAssertEqual(Self.body(request.request)["revocation_token"] as? String, "refresh-4")
                request.respond(200)
            }
        }
        let context = try XCTUnwrap(auth.captureContext())
        let cleanup = Task { await auth.endSession(context: context) }
        await fulfillment(of: [rejected], timeout: 3)
        auth.setServerUrl("https://replacement-deployment.test")
        auth.handleLoginResponse(MobileTokenResponse(
            accessToken: "replacement-access", accessExpiresInSecs: 900,
            refreshToken: "replacement-refresh", refreshExpiresInSecs: 3600,
            tokenType: "Bearer", user: MobileUser(id: "bob", username: "bob", role: "member"),
            revocationToken: "replacement-proof"
        ))
        try XCTUnwrap(log.takeHeld()).respond(401)
        await cleanup.value
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertEqual(auth.user?.id, "bob")
        XCTAssertEqual(auth.getAccessToken(), "replacement-access")
        XCTAssertEqual(KeychainService.loadString(for: KeychainService.refreshTokenKey), "replacement-refresh")
        XCTAssertEqual(KeychainService.loadString(for: KeychainService.revocationTokenKey), "replacement-proof")
        XCTAssertEqual(log.snapshot().count, 2)
        XCTAssertTrue(log.snapshot().allSatisfy { $0.url?.host == "ios-first-upgrade.test" })
        XCTAssertFalse(auth.isCurrent(context), "The old context cannot authenticate as its replacement")
    }

    func testAbsentCurrentValidAndStaleProofsUseOnlyCapturedDeletionCredentials() async throws {
        let proofStates: [String?] = [nil, "refresh-0", "stable-proof", "stale-proof"]
        for storedProof in proofStates {
            let auth = AuthManager()
            auth.clearAuth()
            auth.setServerUrl("https://ios-first-upgrade.test")
            auth.handleLoginResponse(try JSONDecoder.snakeCase.decode(ApiResponse<MobileTokenResponse>.self, from: Self.tokens(0)).data)
            if let storedProof { try KeychainService.saveString(storedProof, for: KeychainService.revocationTokenKey) }
            let log = AuthenticationRequestLog()
            AuthenticationURLProtocol.handler = { request in
                _ = log.append(request.request)
                XCTAssertEqual(request.request.url?.path, "/api/mobile/auth/revoke")
                XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
                let proof = Self.body(request.request)["revocation_token"] as? String
                request.respond(proof == "stale-proof" ? 401 : 200)
            }
            await auth.endSession(context: try XCTUnwrap(auth.captureContext()))
            let expected = storedProof == "stale-proof" ? ["stale-proof", "refresh-0"] : [storedProof ?? "refresh-0"]
            XCTAssertEqual(log.snapshot().compactMap { Self.body($0)["revocation_token"] as? String }, expected)
            XCTAssertFalse(auth.isAuthenticated)
            XCTAssertNil(KeychainService.loadString(for: KeychainService.refreshTokenKey))
        }
    }

}

private extension IOSFirstUpgradeRevocationTests {
    private func seedIOSFirstSession(_ replacements: Int, manager: PushNotificationManager) async throws -> AuthManager {
        let auth = AuthManager()
        auth.clearAuth()
        auth.setServerUrl("https://ios-first-upgrade.test")
        auth.handleLoginResponse(try JSONDecoder.snakeCase.decode(ApiResponse<MobileTokenResponse>.self, from: Self.tokens(0)).data)
        manager.configure(apiClient: APIClient(authManager: auth))
        let log = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in
            let count = log.append(request.request)
            XCTAssertEqual(request.request.url?.path, "/api/mobile/auth/refresh")
            XCTAssertEqual(Self.body(request.request)["refresh_token"] as? String, "refresh-\(count - 1)")
            request.respond(200, data: Self.tokens(count))
        }
        for rotation in 1...replacements {
            _ = try await auth.refreshAccessToken()
            XCTAssertEqual(auth.getAccessToken(), "access-\(rotation)")
            XCTAssertEqual(KeychainService.loadString(for: KeychainService.refreshTokenKey), "refresh-\(rotation)")
            XCTAssertEqual(KeychainService.loadString(for: KeychainService.revocationTokenKey), "refresh-0",
                           "The upgraded app retains a nonempty proof while the baseline Server replaces sessions")
        }
        return auth
    }

    private func prepareUpgradedSession(
        _ replacements: Int, restore: Bool
    ) async throws -> UpgradedSessionFixture {
        let manager = PushNotificationManager()
        var auth = try await seedIOSFirstSession(replacements, manager: manager)
        let log = AuthenticationRequestLog()
        let registered = expectation(description: "registration restored after successful Server-upgrade rotation")
        registered.assertForOverFulfill = false
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            if request.request.url?.path == "/api/mobile/auth/refresh" {
                XCTAssertEqual(Self.body(request.request)["refresh_token"] as? String, "refresh-\(replacements)")
                request.respond(200, data: Self.tokens(replacements + 1))
            } else {
                XCTAssertEqual(request.request.url?.path, "/api/mobile/push/register")
                XCTAssertEqual(request.request.value(forHTTPHeaderField: "Authorization"), "Bearer access-\(replacements + 1)")
                request.respond(200)
                registered.fulfill()
            }
        }
        if restore {
            auth = AuthManager()
            await auth.initialize()
        } else { _ = try await auth.refreshAccessToken() }
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertEqual(auth.getAccessToken(), "access-\(replacements + 1)")
        XCTAssertEqual(KeychainService.loadString(for: KeychainService.revocationTokenKey), "refresh-0")
        let api = APIClient(authManager: auth)
        if restore { manager.configure(apiClient: api) }
        manager.didRegisterForRemoteNotifications(deviceToken: Data([1, 2]))
        await fulfillment(of: [registered], timeout: 3)
        XCTAssertEqual(log.snapshot().filter { $0.url?.path == "/api/mobile/push/register" }.count, 1)
        return UpgradedSessionFixture(auth: auth, api: api, manager: manager)
    }

    nonisolated static func tokens(_ rotation: Int) -> Data {
        Data("""
        {"data":{"access_token":"access-\(rotation)","access_expires_in_secs":900,
        "refresh_token":"refresh-\(rotation)","refresh_expires_in_secs":3600,"token_type":"Bearer",
        "user":{"id":"alice","username":"alice","role":"member"}}}
        """.utf8)
    }

    nonisolated static func body(_ request: URLRequest) -> [String: Any] {
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
