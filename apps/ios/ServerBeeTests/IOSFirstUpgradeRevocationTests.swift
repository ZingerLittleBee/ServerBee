import Foundation
import XCTest
@testable import ServerBee

private struct UpgradedSessionFixture {
    let auth: AuthManager
    let api: APIClient
    let manager: PushNotificationManager
    let context: MobileAuthenticationContext
}

/// The old Server's HTTP boundary replaces its session on every refresh, as
/// pinned baseline 923a255 does. Client persistence and all cleanup are real.
@MainActor
final class IOSFirstUpgradeRevocationTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(AuthenticationURLProtocol.self) }

    override func tearDown() async throws {
        AuthenticationURLProtocol.handler = nil
        AuthenticationURLProtocol.cancelPending()
        URLProtocol.unregisterClass(AuthenticationURLProtocol.self)
        AuthManager().clearAuth()
        try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
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
                XCTAssertEqual((try? AuthManager.readAuthentication())?.refreshToken, capturedSecret,
                               "Keep the captured original credential until cleanup succeeds")
                request.respond(count == 1 ? 401 : 200)
                if count == 2 { revoked.fulfill() }
            case "/api/servers", "/api/mobile/push/unregister", "/api/mobile/push/register": request.respond(401)
            default: XCTFail("Stale proof recovery must not enter bearer logout or ordinary authentication"); request.respond(401)
            }
        }
        let subject: AuthManager
        if route == "startup" {
            subject = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [AuthenticationURLProtocol.self]))
            await subject.initialize()
        } else {
            subject = auth
            do {
                _ = try await auth.refreshAccessToken()
                XCTFail("Expected committed response loss")
            } catch AuthError.refreshNetworkFailure { /* The transport failure preserves the original login. */ }
            XCTAssertTrue(auth.isAuthenticated)
            XCTAssertEqual((try? AuthManager.readAuthentication())?.revocationToken, "refresh-0")
            if route == "api" {
                do {
                    let _: String = try await api.get("/api/servers")
                    XCTFail("Expected unauthorized")
                } catch APIError.unauthorized { /* Production automatic cleanup recovers the captured proof. */ }
            } else if route == "push" {
                // This legacy request holds its pre-upgrade push context. The
                // current client must not create an encrypted registration without bootstrap.
                do {
                    try await api.postVoid("/api/mobile/push/register", body: ["device_token": "legacy-token"], context: fixture.context)
                    XCTFail("Expected unauthorized for the captured legacy push request")
                } catch APIError.unauthorized { /* Cleanup uses this login's current fallback secret. */ }
            } else {
                await SettingsViewModel().logout(
                    authManager: auth, unregisterPush: manager.unregister(context:), closeWebSocket: {}
                )
            }
        }
        await fulfillment(of: [revoked], timeout: 3)
        XCTAssertFalse(subject.isAuthenticated)
        XCTAssertNil(subject.getAccessToken())
        XCTAssertNil(try AuthManager.readAuthentication())
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
            await SettingsViewModel().logout(authManager: auth, unregisterPush: manager.unregister(context:), closeWebSocket: {})
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
        XCTAssertNil(try AuthManager.readAuthentication())
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
        auth.clearAuth()
        auth.setServerUrl("https://replacement-deployment.test")
        auth.handleLoginResponse(MobileTokenResponse(
            accessToken: "replacement-access", accessExpiresInSecs: 900,
            refreshToken: "replacement-refresh", refreshExpiresInSecs: 3600,
            tokenType: "Bearer", user: MobileUser(id: "bob", username: "bob", role: "member"),
            revocationToken: "replacement-proof", mobileSessionId: "22222222-2222-4222-8222-222222222222"
        ))
        try XCTUnwrap(log.takeHeld()).respond(401)
        await cleanup.value
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertEqual(auth.user?.id, "bob")
        XCTAssertEqual(auth.getAccessToken(), "replacement-access")
        XCTAssertEqual((try? AuthManager.readAuthentication())?.refreshToken, "replacement-refresh")
        XCTAssertEqual((try? AuthManager.readAuthentication())?.revocationToken, "replacement-proof")
        XCTAssertEqual(log.snapshot().count, 2)
        XCTAssertTrue(log.snapshot().allSatisfy { $0.url?.host == "ios-first-upgrade.test" })
        XCTAssertFalse(auth.isCurrent(context), "The old context cannot authenticate as its replacement")
    }

    func testAbsentCurrentValidAndStaleProofsUseOnlyCapturedDeletionCredentials() async throws {
        let proofStates: [String?] = [nil, "refresh-0", "stable-proof", "stale-proof"]
        for storedProof in proofStates {
            let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [AuthenticationURLProtocol.self]))
            auth.clearAuth()
            auth.setServerUrl("https://ios-first-upgrade.test")
            auth.handleLoginResponse(MobileTokenResponse(
                accessToken: "access-0", accessExpiresInSecs: 900, refreshToken: "refresh-0", refreshExpiresInSecs: 3600,
                tokenType: "Bearer", user: MobileUser(id: "alice", username: "alice", role: "member"), revocationToken: storedProof
            ))
            XCTAssertNil((try AuthManager.readAuthentication())?.confirmedDeletionProof)
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
            XCTAssertNil(try AuthManager.readAuthentication())
        }
    }

}

private extension IOSFirstUpgradeRevocationTests {
    private func seedIOSFirstSession(_ replacements: Int, manager: PushNotificationManager) async throws -> (AuthManager, MobileAuthenticationContext) {
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [AuthenticationURLProtocol.self]))
        auth.clearAuth()
        auth.setServerUrl("https://ios-first-upgrade.test")
        auth.handleLoginResponse(try JSONDecoder.snakeCase.decode(ApiResponse<MobileTokenResponse>.self, from: Self.tokens(0)).data)
        manager.configure(apiClient: APIClient(authManager: auth))
        let context = try XCTUnwrap(auth.captureContext())
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
            XCTAssertEqual((try? AuthManager.readAuthentication())?.refreshToken, "refresh-\(rotation)")
            XCTAssertEqual((try? AuthManager.readAuthentication())?.revocationToken, "refresh-0",
                           "The upgraded app retains a nonempty proof while the baseline Server replaces sessions")
        }
        return (auth, context)
    }

    private func prepareUpgradedSession(
        _ replacements: Int, restore: Bool
    ) async throws -> UpgradedSessionFixture {
        let manager = PushNotificationManager(system: TestPushSystem(), storage: MemoryPushSetupStorage())
        let (original, context) = try await seedIOSFirstSession(replacements, manager: manager)
        var auth = original
        let log = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            if request.request.url?.path == "/api/mobile/auth/refresh" {
                XCTAssertEqual(Self.body(request.request)["refresh_token"] as? String, "refresh-\(replacements)")
                request.respond(200, data: Self.tokens(replacements + 1))
            } else if request.request.url?.path == "/api/mobile/push/settings" {
                request.respond(200, data: PushSetupTestData.response())
            } else {
                XCTFail("Unknown legacy proof cannot create a verified registration")
                request.respond(400)
            }
        }
        if restore {
            auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [AuthenticationURLProtocol.self]))
            await auth.initialize()
        } else { _ = try await auth.refreshAccessToken() }
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertEqual(auth.getAccessToken(), "access-\(replacements + 1)")
        XCTAssertEqual((try? AuthManager.readAuthentication())?.revocationToken, "refresh-0")
        let api = APIClient(authManager: auth)
        if restore { manager.configure(apiClient: api) }
        await manager.reconcile()
        XCTAssertNil((try AuthManager.readAuthentication())?.confirmedDeletionProof)
        XCTAssertTrue(try PendingSessionRevocations().records().isEmpty)
        XCTAssertFalse(log.snapshot().contains { $0.url?.path == "/api/mobile/push/encrypted-register" })
        return UpgradedSessionFixture(auth: auth, api: api, manager: manager, context: context)
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
