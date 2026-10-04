import Foundation
import XCTest
@testable import ServerBee

@MainActor
private final class RecoveryAuthenticationStorage: MobileAuthenticationStorage {
    var value: SavedMobileAuthentication?
    var failDelete = false
    func read() throws -> SavedMobileAuthentication? { value }
    func write(_ value: SavedMobileAuthentication) throws { self.value = value }
    func delete() throws {
        if failDelete { throw KeychainError.encodingFailed }
        value = nil
    }
}

@MainActor
private final class RecoveryRevocationStorage: SessionRevocationStorage {
    var values: [PendingSessionRevocation] = []
    func read() throws -> [PendingSessionRevocation] { values }
    func write(_ records: [PendingSessionRevocation]) throws { values = records }
}

@MainActor
final class SessionRecoveryTests: XCTestCase {
    private let normal = RecoveryAuthenticationStorage()
    private let journal = RecoveryRevocationStorage()
    private let session = APIClient.makeCleanupSession(protocolClasses: [AuthenticationURLProtocol.self])

    override func setUp() async throws {
        URLProtocol.registerClass(AuthenticationURLProtocol.self)
        normal.value = SavedMobileAuthentication(loginId: UUID(), serverUrl: "https://recovery.test",
            installationId: "original-installation", user: MobileUser(id: "alice", username: "alice", role: "member"),
            accessToken: "old-access", refreshToken: "old-refresh")
    }

    override func tearDown() async throws {
        AuthenticationURLProtocol.handler = nil
        AuthenticationURLProtocol.cancelPending()
        session.invalidateAndCancel()
        URLProtocol.unregisterClass(AuthenticationURLProtocol.self)
        AuthManager().clearAuth()
    }

    private func manager() -> AuthManager {
        AuthManager(revocations: PendingSessionRevocations(storage: journal), authenticationStorage: normal, cleanupSession: session)
    }

    private func suspendedManager() async -> AuthManager {
        AuthenticationURLProtocol.handler = { $0.respond(401) }
        let auth = manager()
        await auth.initialize()
        XCTAssertNotNil(auth.sessionRecovery)
        XCTAssertFalse(auth.isAuthenticated)
        return auth
    }

    nonisolated private static func acknowledgement(_ outcome: String = "already_absent", user: String = "alice",
                                                   installation: String = "original-installation", sessionId: String? = nil) -> Data {
        let session = sessionId.map { "\"\($0)\"" } ?? "null"
        return Data("""
        {"data":{"outcome":"\(outcome)","user_id":"\(user)","installation_id":"\(installation)","mobile_session_id":\(session)}}
        """.utf8)
    }

    nonisolated private static var selection: Data {
        Data("""
        {"data":{"outcome":"selection_required","user_id":"alice","installation_id":"original-installation",
        "mobile_session_id":null,"candidates":[{"mobile_session_id":"11111111-1111-4111-8111-111111111111",
        "device_name":"Original iPhone","created_at":"2026-09-01T00:00:00Z","last_used_at":"2026-09-02T00:00:00Z"},
        {"mobile_session_id":"22222222-2222-4222-8222-222222222222","device_name":"Another login",
        "created_at":"2026-09-03T00:00:00Z","last_used_at":"2026-09-04T00:00:00Z"}]}}
        """.utf8)
    }

    func testPermanentLegacyRejectionSuspendsOrdinaryRequestsAcrossRestart() async throws {
        let log = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            request.respond(401)
        }
        let auth = manager()
        await auth.initialize()
        XCTAssertFalse(auth.isAuthenticated, "Rejected legacy credentials must leave the normal authenticated UI")
        XCTAssertNil(auth.captureContext(), "Ordinary HTTP and WebSocket requests must be suspended")
        XCTAssertNotNil(normal.value, "Recovery must preserve the only original identity")
        XCTAssertTrue(journal.values.isEmpty, "Unconfirmed legacy material is never a durable deletion proof")
        do {
            try await auth.prepareForLogin()
            XCTFail("Normal login must still wait for confirmed cleanup")
        } catch AuthError.secureLogoutNeedsConnection { /* Expected recovery gate. */ }

        let count = log.snapshot().count
        let restarted = manager()
        await restarted.initialize()
        XCTAssertFalse(restarted.isAuthenticated)
        XCTAssertNil(restarted.captureContext())
        XCTAssertEqual(log.snapshot().count, count, "A cold restart must not retry permanently rejected normal credentials")
    }

    func testReauthenticationUsesOnlyOriginalIdentityAndConfirmedAbsenceReleasesLogin() async throws {
        let auth = await suspendedManager()
        AuthenticationURLProtocol.handler = { request in
            XCTAssertEqual(request.request.url?.absoluteString, "https://recovery.test/api/mobile/auth/recover")
            XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
            XCTAssertNil(request.request.value(forHTTPHeaderField: "Cookie"))
            let body = PushSetupTestData.body(request.request)
            XCTAssertEqual(body["username"] as? String, "alice")
            XCTAssertEqual(body["password"] as? String, "fixture-password")
            XCTAssertEqual(body["expected_user_id"] as? String, "alice")
            XCTAssertEqual(body["installation_id"] as? String, "original-installation")
            XCTAssertEqual(body["access_token"] as? String, "old-access")
            XCTAssertEqual(body["refresh_token"] as? String, "old-refresh")
            XCTAssertNil(body["expected_session_id"])
            request.respond(200, data: Self.acknowledgement())
        }
        try await auth.recoverSession(username: "alice", password: "fixture-password")
        XCTAssertNil(normal.value)
        XCTAssertNil(auth.sessionRecovery)
        XCTAssertFalse(auth.isAuthenticated, "Cleanup never publishes a replacement login")
        XCTAssertTrue(journal.values.isEmpty)
        try await auth.prepareForLogin()
    }

    func testFailedOrUnscopedRecoveryNeverDiscardsOriginalIdentity() async throws {
        let auth = await suspendedManager()
        let id = try XCTUnwrap(normal.value?.loginId)
        let failures: [(Int, Data)] = [
            (401, Data()), (404, Data()), (409, Data()), (429, Data()), (500, Data()), (307, Data()),
            (200, Data(#"{"data":"ok"}"#.utf8)),
            (200, Self.acknowledgement(user: "bob")),
            (200, Self.acknowledgement(installation: "other-installation")),
            (200, Self.acknowledgement("ok")),
            (200, Self.acknowledgement(sessionId: "11111111-1111-4111-8111-111111111111")),
            (200, Self.acknowledgement("ok", sessionId: "not-a-session"))
        ]
        for (status, data) in failures {
            AuthenticationURLProtocol.handler = { $0.respond(status, data: data) }
            do {
                try await auth.recoverSession(username: "alice", password: "fixture-password")
                XCTFail("HTTP \(status) must not count as confirmed cleanup")
            } catch { /* The exact identity remains recoverable. */ }
            XCTAssertEqual(normal.value?.loginId, id)
            XCTAssertNotNil(auth.sessionRecovery)
            XCTAssertNil(auth.captureContext())
        }
        AuthenticationURLProtocol.handler = { $0.loseResponse() }
        do {
            try await auth.recoverSession(username: "alice", password: "fixture-password")
            XCTFail("A lost response must retain the original identity")
        } catch { /* Can retry safely. */ }
        XCTAssertEqual(normal.value?.loginId, id)
    }

    func testKnownSessionRequiresAnExactSessionAcknowledgement() async throws {
        let known = "11111111-1111-4111-8111-111111111111"
        normal.value?.mobileSessionId = known
        let auth = await suspendedManager()
        AuthenticationURLProtocol.handler = { request in
            XCTAssertEqual(PushSetupTestData.body(request.request)["expected_session_id"] as? String, known)
            request.respond(200, data: Self.acknowledgement("ok", sessionId: "22222222-2222-4222-8222-222222222222"))
        }
        do {
            try await auth.recoverSession(username: "alice", password: "fixture-password")
            XCTFail("Another session's acknowledgement cannot release this one")
        } catch SessionRecoveryError.invalidConfirmation { /* Expected scope rejection. */ }
        XCTAssertNotNil(normal.value)
        AuthenticationURLProtocol.handler = { $0.respond(200, data: Self.acknowledgement("ok", sessionId: known)) }
        try await auth.recoverSession(username: "alice", password: "fixture-password")
        XCTAssertNil(normal.value)
    }

    func testRecoveryWriteFailurePreservesSnapshotAndColdRecoveryState() async throws {
        let auth = await suspendedManager()
        normal.failDelete = true
        AuthenticationURLProtocol.handler = { $0.respond(200, data: Self.acknowledgement()) }
        do {
            try await auth.recoverSession(username: "alice", password: "fixture-password")
            XCTFail("Failed local cleanup cannot publish a normal login")
        } catch { /* Server absence can be confirmed again. */ }
        XCTAssertNotNil(normal.value)
        let restarted = manager()
        AuthenticationURLProtocol.handler = { request in XCTFail("No ordinary auth traffic during persisted recovery"); request.respond(401) }
        await restarted.initialize()
        XCTAssertNotNil(restarted.sessionRecovery)
        XCTAssertNil(restarted.getAccessToken())
        normal.failDelete = false
        AuthenticationURLProtocol.handler = { $0.respond(200, data: Self.acknowledgement()) }
        try await restarted.recoverSession(username: "alice", password: "fixture-password")
        XCTAssertNil(normal.value)
    }

    func testDelayedRecoveryCannotClearAReplacementLogin() async throws {
        let auth = await suspendedManager()
        let log = AuthenticationRequestLog()
        let sent = expectation(description: "old recovery in flight")
        AuthenticationURLProtocol.handler = { request in log.hold(request); sent.fulfill() }
        let recovery = Task { try await auth.recoverSession(username: "alice", password: "fixture-password") }
        await fulfillment(of: [sent], timeout: 3)
        auth.clearAuth()
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "replacement-access", accessExpiresInSecs: 900,
            refreshToken: "replacement-refresh", refreshExpiresInSecs: 3600, tokenType: "Bearer",
            user: MobileUser(id: "bob", username: "bob", role: "member"), revocationToken: "replacement-proof",
            mobileSessionId: "22222222-2222-4222-8222-222222222222"), installationId: "replacement-installation")
        try XCTUnwrap(log.takeHeld()).respond(200, data: Self.acknowledgement())
        do { _ = try await recovery.value; XCTFail("Late old cleanup cannot own the replacement identity") } catch AuthError.staleIdentity { /* Expected generation fence. */ }
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertEqual(auth.user?.id, "bob")
        XCTAssertEqual(normal.value?.accessToken, "replacement-access")
    }

    func testSettingsLogoutWithoutContextCannotDiscardRecoveryIdentity() async {
        let auth = await suspendedManager()
        await SettingsViewModel().logout(authManager: auth, unregisterPush: { _ in }, closeWebSocket: {})
        XCTAssertNotNil(normal.value)
        XCTAssertNotNil(auth.sessionRecovery)
    }

    func testPermanentRejectionSuspendsRequestsBeforeCleanupResponds() async throws {
        let auth = manager()
        let log = AuthenticationRequestLog()
        let cleanupSent = expectation(description: "revoke waiting for response")
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            if request.request.url?.path == "/api/mobile/auth/revoke" {
                log.hold(request)
                cleanupSent.fulfill()
            } else { request.respond(401) }
        }
        let startup = Task { await auth.initialize() }
        await fulfillment(of: [cleanupSent], timeout: 3)
        XCTAssertNil(auth.captureContext())
        XCTAssertNil(auth.getAccessToken())
        XCTAssertNotNil(auth.sessionRecovery)
        XCTAssertEqual(normal.value?.requiresSessionRecovery, true)
        let count = log.snapshot().count
        do {
            let _: String = try await APIClient(authManager: auth).get("/api/servers")
            XCTFail("Ordinary requests must stop before pending cleanup returns")
        } catch APIError.unauthorized { /* No HTTP dispatch. */ }
        XCTAssertEqual(log.snapshot().count, count)
        try XCTUnwrap(log.takeHeld()).respond(401)
        await startup.value
        XCTAssertNotNil(normal.value)
    }

    func testKnownIdentityRetryKeepsExactTargetAndCannotFallBackToLogout() async throws {
        let known = "11111111-1111-4111-8111-111111111111"
        normal.value?.mobileSessionId = known
        let auth = await suspendedManager()
        let log = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            XCTAssertEqual(request.request.url?.path, "/api/mobile/auth/revoke")
            XCTAssertEqual(PushSetupTestData.body(request.request)["expected_session_id"] as? String, known)
            request.respond(401)
        }
        do {
            try await auth.retrySessionCleanup()
            XCTFail("Rejected exact-target cleanup must preserve the identity")
        } catch { /* No unscoped logout fallback. */ }
        XCTAssertNotNil(normal.value)
        XCTAssertFalse(log.snapshot().contains { $0.url?.path == "/api/mobile/auth/logout" })
        AuthenticationURLProtocol.handler = { $0.respond(200, data: Data(#"{"data":"unconfirmed"}"#.utf8)) }
        do { try await auth.retrySessionCleanup(); XCTFail("Malformed cleanup cannot release the identity") } catch { }
        XCTAssertNotNil(normal.value)
        AuthenticationURLProtocol.handler = { $0.respond(200) }
        try await auth.retrySessionCleanup()
        XCTAssertNil(normal.value)
        XCTAssertNil(auth.sessionRecovery)
    }

    func testLateRefreshCannotReviveASuspendedIdentity() async throws {
        let auth = manager()
        // Publish the same synthetic legacy identity before exercising overlap.
        auth.clearAuth()
        auth.setServerUrl("https://recovery.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "old-access", accessExpiresInSecs: 900,
            refreshToken: "old-refresh", refreshExpiresInSecs: 3600, tokenType: "Bearer",
            user: MobileUser(id: "alice", username: "alice", role: "member")), installationId: "original-installation")
        let context = try XCTUnwrap(auth.captureContext())
        let log = AuthenticationRequestLog()
        let refreshSent = expectation(description: "refresh waiting for response")
        AuthenticationURLProtocol.handler = { request in
            if request.request.url?.path == "/api/mobile/auth/refresh" { log.hold(request); refreshSent.fulfill() }
            else { request.respond(401) }
        }
        let refresh = Task { try await auth.refreshAccessToken(context: context) }
        await fulfillment(of: [refreshSent], timeout: 3)
        await auth.endSession(context: context, authenticationRejected: true)
        try XCTUnwrap(log.takeHeld()).respond(200, data: Data("""
        {"data":{"access_token":"late-access","access_expires_in_secs":900,"refresh_token":"late-refresh",
        "refresh_expires_in_secs":3600,"token_type":"Bearer","user":{"id":"alice","username":"alice","role":"member"}}}
        """.utf8))
        do { _ = try await refresh.value; XCTFail("Suspended identity must reject late rotation") }
        catch AuthError.staleIdentity { /* The saved original stays recoverable. */ }
        XCTAssertNotNil(auth.sessionRecovery)
        XCTAssertEqual(normal.value?.accessToken, "old-access")
        XCTAssertNil(auth.getAccessToken())
    }

    func testPrivateSnapshotRestoresRecoveryWithoutOrdinaryTraffic() async throws {
        var original = try XCTUnwrap(normal.value)
        original.requiresSessionRecovery = true
        try PrivateMobileAuthenticationStorage().write(original)
        AuthenticationURLProtocol.handler = { request in XCTFail("Persisted recovery must not refresh or sign out automatically"); request.respond(401) }
        let auth = AuthManager(revocations: PendingSessionRevocations(storage: journal), cleanupSession: session)
        await auth.initialize()
        XCTAssertEqual(auth.sessionRecovery?.loginId, original.loginId)
        XCTAssertEqual(auth.sessionRecovery?.serverUrl, original.serverUrl)
        XCTAssertNil(auth.captureContext())
        XCTAssertEqual(try AuthManager.readAuthentication()?.refreshToken, "old-refresh")
    }

    func testExplicitSelectionPersistsExactTargetBeforeLostResponseAndColdRetry() async throws {
        let auth = await suspendedManager()
        AuthenticationURLProtocol.handler = { $0.respond(200, data: Self.selection) }
        let candidates = try await auth.recoverSession(username: "alice", password: "fixture-password")
        XCTAssertEqual(candidates.count, 2)
        XCTAssertNil(normal.value?.selectedRecoverySessionId, "Reauthentication never guesses a target")
        XCTAssertNotNil(auth.sessionRecovery)
        AuthenticationURLProtocol.handler = { request in XCTFail("A candidate not returned by the server must not be sent"); request.respond(200) }
        do {
            try await auth.recoverSession(username: "alice", password: "fixture-password",
                selectedSessionId: "33333333-3333-4333-8333-333333333333")
            XCTFail("Unverified candidate must not become a target")
        } catch SessionRecoveryError.invalidConfirmation { /* Original untouched. */ }

        let chosen = candidates[0].mobileSessionId
        let log = AuthenticationRequestLog()
        let selectedSent = expectation(description: "selected recovery in flight")
        AuthenticationURLProtocol.handler = { request in
            XCTAssertEqual(PushSetupTestData.body(request.request)["expected_session_id"] as? String, chosen)
            log.hold(request)
            selectedSent.fulfill()
        }
        let recovery = Task { try await auth.recoverSession(username: "alice", password: "fixture-password", selectedSessionId: chosen) }
        await fulfillment(of: [selectedSent], timeout: 3)
        XCTAssertEqual(normal.value?.selectedRecoverySessionId, chosen)
        XCTAssertNil(normal.value?.mobileSessionId, "User selection does not forge original proof provenance")
        XCTAssertTrue(journal.values.isEmpty)
        try XCTUnwrap(log.takeHeld()).loseResponse()
        do { _ = try await recovery.value; XCTFail("Lost response cannot release the saved identity") } catch { }

        let restarted = manager()
        AuthenticationURLProtocol.handler = { request in XCTFail("Cold recovery stays suspended"); request.respond(401) }
        await restarted.initialize()
        XCTAssertEqual(restarted.sessionRecovery?.selectedSessionId, chosen)
        AuthenticationURLProtocol.handler = { request in
            XCTAssertEqual(PushSetupTestData.body(request.request)["expected_session_id"] as? String, chosen)
            request.respond(200, data: Self.acknowledgement(sessionId: chosen))
        }
        try await restarted.recoverSession(username: "alice", password: "fixture-password")
        XCTAssertNil(normal.value)
    }

    func testSelectionViewModelWaitsForUserChoiceAndRetainsReauthenticationFields() async throws {
        let auth = await suspendedManager()
        let viewModel = SessionRecoveryViewModel()
        viewModel.username = "alice"
        viewModel.password = "fixture-password"
        AuthenticationURLProtocol.handler = { $0.respond(200, data: Self.selection) }
        await viewModel.recover(authManager: auth)
        XCTAssertEqual(viewModel.candidates.count, 2)
        XCTAssertNil(viewModel.selectedSessionId)
        XCTAssertEqual(viewModel.password, "fixture-password")
        AuthenticationURLProtocol.handler = { request in XCTFail("No dispatch without an explicit choice"); request.respond(200) }
        await viewModel.recover(authManager: auth)
        XCTAssertNil(normal.value?.selectedRecoverySessionId)
        viewModel.selectedSessionId = viewModel.candidates[0].mobileSessionId
        AuthenticationURLProtocol.handler = { $0.respond(200, data: Self.acknowledgement("ok", sessionId: "11111111-1111-4111-8111-111111111111")) }
        await viewModel.recover(authManager: auth)
        XCTAssertNil(normal.value)
        XCTAssertTrue(viewModel.password.isEmpty)
        XCTAssertTrue(viewModel.candidates.isEmpty)
    }

    func testRecoveryViewModelRequestsTOTPWithoutCreatingALogin() async throws {
        let auth = await suspendedManager()
        let viewModel = SessionRecoveryViewModel()
        viewModel.username = "alice"
        viewModel.password = "fixture-password"
        AuthenticationURLProtocol.handler = { $0.respond(422, data: Data(#"{"error":{"message":"Validation error: 2fa_required"}}"#.utf8)) }
        await viewModel.recover(authManager: auth)
        XCTAssertTrue(viewModel.requiresTOTP)
        XCTAssertFalse(viewModel.isWorking)
        XCTAssertNotNil(normal.value)
        viewModel.totpCode = "123456"
        AuthenticationURLProtocol.handler = { request in
            XCTAssertEqual(PushSetupTestData.body(request.request)["totp_code"] as? String, "123456")
            request.respond(200, data: Self.acknowledgement())
        }
        await viewModel.recover(authManager: auth)
        XCTAssertNil(viewModel.errorMessage)
        XCTAssertTrue(viewModel.password.isEmpty)
        XCTAssertTrue(viewModel.totpCode.isEmpty)
        XCTAssertNil(normal.value)
        XCTAssertFalse(auth.isAuthenticated)
    }
}
