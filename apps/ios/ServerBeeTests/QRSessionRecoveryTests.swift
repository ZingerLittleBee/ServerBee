import Foundation
import XCTest
@testable import ServerBee

@MainActor
private final class QRRecoveryAuthenticationStorage: MobileAuthenticationStorage {
    var value: SavedMobileAuthentication?
    func read() throws -> SavedMobileAuthentication? { value }
    func write(_ value: SavedMobileAuthentication) throws { self.value = value }
    func delete() throws { value = nil }
}

@MainActor
private final class QRRecoveryRevocationStorage: SessionRevocationStorage {
    var values: [PendingSessionRevocation] = []
    func read() throws -> [PendingSessionRevocation] { values }
    func write(_ values: [PendingSessionRevocation]) throws { self.values = values }
}

@MainActor
final class QRSessionRecoveryTests: XCTestCase {
    private let normal = QRRecoveryAuthenticationStorage()
    private let journal = QRRecoveryRevocationStorage()
    private let session = APIClient.makeCleanupSession(protocolClasses: [AuthenticationURLProtocol.self])
    nonisolated private static let target = "11111111-1111-4111-8111-111111111111"

    override func setUp() async throws {
        URLProtocol.registerClass(AuthenticationURLProtocol.self)
        normal.value = SavedMobileAuthentication(loginId: UUID(), serverUrl: "https://recovery.test",
            installationId: "original-installation", user: MobileUser(id: "alice", username: "alice", role: "member"),
            accessToken: "old-access", refreshToken: "old-refresh", requiresSessionRecovery: true)
    }

    override func tearDown() async throws {
        AuthenticationURLProtocol.handler = nil
        AuthenticationURLProtocol.cancelPending()
        session.invalidateAndCancel()
        URLProtocol.unregisterClass(AuthenticationURLProtocol.self)
        AuthManager().clearAuth()
    }

    private func manager() async -> AuthManager {
        let auth = AuthManager(revocations: PendingSessionRevocations(storage: journal), authenticationStorage: normal, cleanupSession: session)
        AuthenticationURLProtocol.handler = { request in XCTFail("Recovery restart must not use ordinary authentication"); request.respond(401) }
        await auth.initialize()
        XCTAssertNotNil(auth.sessionRecovery)
        return auth
    }

    nonisolated private static func acknowledgement(target: String? = nil) -> Data {
        let session = target.map { "\"\($0)\"" } ?? "null"
        return Data("""
        {"data":{"outcome":"already_absent","user_id":"alice","installation_id":"original-installation",
        "mobile_session_id":\(session),"recovery_token":"sb_recover_fixture"}}
        """.utf8)
    }

    nonisolated private static var selection: Data {
        Data("""
        {"data":{"outcome":"selection_required","user_id":"alice","installation_id":"original-installation",
        "mobile_session_id":null,"recovery_token":"sb_recover_fixture","candidates":[
        {"mobile_session_id":"11111111-1111-4111-8111-111111111111","device_name":"Original iPhone",
        "created_at":"2026-09-01T00:00:00Z","last_used_at":"2026-09-02T00:00:00Z"},
        {"mobile_session_id":"22222222-2222-4222-8222-222222222222","device_name":"Another login",
        "created_at":"2026-09-03T00:00:00Z","last_used_at":"2026-09-04T00:00:00Z"}]}}
        """.utf8)
    }

    func testQRCodeRecoversOriginalWithoutPasswordOrReplacementLogin() async throws {
        let auth = await manager()
        AuthenticationURLProtocol.handler = { request in
            XCTAssertEqual(request.request.url?.absoluteString, "https://recovery.test/api/mobile/auth/recover")
            XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
            XCTAssertNil(request.request.value(forHTTPHeaderField: "Cookie"))
            let body = PushSetupTestData.body(request.request)
            XCTAssertEqual(body["pairing_code"] as? String, "sb_pair_fixture")
            XCTAssertNil(body["username"])
            XCTAssertNil(body["password"])
            XCTAssertNil(body["totp_code"])
            XCTAssertNil(body["recovery_token"])
            XCTAssertEqual(body["expected_user_id"] as? String, "alice")
            XCTAssertEqual(body["installation_id"] as? String, "original-installation")
            XCTAssertEqual(body["access_token"] as? String, "old-access")
            XCTAssertEqual(body["refresh_token"] as? String, "old-refresh")
            request.respond(200, data: Self.acknowledgement())
        }
        let payload = try XCTUnwrap(PairingQRCode.decode(#"{"type":"serverbee_pair","server_url":"https://RECOVERY.test:443/","code":"sb_pair_fixture"}"#))
        let viewModel = SessionRecoveryViewModel()
        await viewModel.recoverWithQRCode(serverUrl: payload.serverUrl, code: payload.code, authManager: auth)
        XCTAssertNil(viewModel.errorMessage)
        XCTAssertNil(normal.value)
        XCTAssertNil(auth.sessionRecovery)
        XCTAssertFalse(auth.isAuthenticated, "Recovery must return to normal login without publishing QR auth tokens")
        XCTAssertTrue(journal.values.isEmpty)
        try await auth.prepareForLogin()
    }

    func testScannedServerMismatchNeverDispatchesCapturedCredentials() async throws {
        let auth = await manager()
        AuthenticationURLProtocol.handler = { request in XCTFail("A foreign QR URL must never receive captured credentials"); request.respond(200) }
        for server in ["https://attacker.test", "http://recovery.test", "https://recovery.test:8443",
                       "https://recovery.test/another-base", "https://user@recovery.test", "https://recovery.test?redirect=1",
                       "https://recovery.test#fragment", "file:///recovery.test"] {
            do {
                _ = try await auth.recoverSession(serverUrl: server, pairingCode: "sb_pair_fixture")
                XCTFail("Mismatched QR must fail: \(server)")
            } catch SessionRecoveryError.mismatchedQRServer { /* No request and original retained. */ }
        }
        XCTAssertNotNil(normal.value)
        XCTAssertNil(auth.captureContext())
    }

    func testQRSelectionRequiresExplicitChoiceAndUsesCleanupOnlyGrant() async throws {
        let auth = await manager()
        let viewModel = SessionRecoveryViewModel()
        AuthenticationURLProtocol.handler = { $0.respond(200, data: Self.selection) }
        await viewModel.recoverWithQRCode(serverUrl: "https://recovery.test", code: "sb_pair_fixture", authManager: auth)
        XCTAssertEqual(viewModel.candidates.count, 2)
        XCTAssertNil(viewModel.selectedSessionId)
        XCTAssertFalse(viewModel.canConfirmQRSelection)
        XCTAssertNil(normal.value?.selectedRecoverySessionId)
        AuthenticationURLProtocol.handler = { request in XCTFail("No cleanup request before explicit user choice"); request.respond(200) }
        await viewModel.confirmQRSelection(authManager: auth)
        viewModel.selectedSessionId = Self.target
        XCTAssertTrue(viewModel.canConfirmQRSelection)
        AuthenticationURLProtocol.handler = { request in
            let body = PushSetupTestData.body(request.request)
            XCTAssertEqual(body["recovery_token"] as? String, "sb_recover_fixture")
            XCTAssertEqual(body["expected_session_id"] as? String, Self.target)
            XCTAssertNil(body["pairing_code"])
            XCTAssertNil(body["password"])
            XCTAssertNil(body["username"])
            request.respond(200, data: Self.acknowledgement(target: Self.target))
        }
        await viewModel.confirmQRSelection(authManager: auth)
        XCTAssertNil(viewModel.errorMessage)
        XCTAssertNil(normal.value)
        XCTAssertFalse(viewModel.canConfirmQRSelection)
    }

    func testLostQRSelectionResponsePersistsExactTargetAndColdRestartRequiresFreshScan() async throws {
        let auth = await manager()
        let viewModel = SessionRecoveryViewModel()
        AuthenticationURLProtocol.handler = { $0.respond(200, data: Self.selection) }
        await viewModel.recoverWithQRCode(serverUrl: "https://recovery.test", code: "sb_pair_fixture", authManager: auth)
        viewModel.selectedSessionId = Self.target
        let held = AuthenticationRequestLog()
        let sent = expectation(description: "QR selected cleanup in flight")
        AuthenticationURLProtocol.handler = { request in held.hold(request); sent.fulfill() }
        let cleanup = Task { await viewModel.confirmQRSelection(authManager: auth) }
        await fulfillment(of: [sent], timeout: 3)
        XCTAssertEqual(normal.value?.selectedRecoverySessionId, Self.target, "Persist original target before dispatch")
        try XCTUnwrap(held.takeHeld()).loseResponse()
        await cleanup.value
        XCTAssertNotNil(normal.value)
        let restarted = await manager()
        let freshViewModel = SessionRecoveryViewModel()
        XCTAssertFalse(freshViewModel.canConfirmQRSelection, "QR grant is never restored from auth storage")
        AuthenticationURLProtocol.handler = { request in
            let body = PushSetupTestData.body(request.request)
            XCTAssertEqual(body["pairing_code"] as? String, "sb_pair_fresh")
            XCTAssertEqual(body["expected_session_id"] as? String, Self.target)
            XCTAssertNil(body["recovery_token"])
            request.respond(200, data: Self.acknowledgement(target: Self.target))
        }
        await freshViewModel.recoverWithQRCode(serverUrl: "https://recovery.test", code: "sb_pair_fresh", authManager: restarted)
        XCTAssertNil(normal.value)
        XCTAssertFalse(restarted.isAuthenticated)
    }

    func testExpiredQRGrantDisablesConfirmationWithoutDiscardingOriginal() async throws {
        let auth = await manager()
        let viewModel = SessionRecoveryViewModel()
        AuthenticationURLProtocol.handler = { $0.respond(200, data: Self.selection) }
        await viewModel.recoverWithQRCode(serverUrl: "https://recovery.test", code: "sb_pair_fixture", authManager: auth)
        viewModel.selectedSessionId = Self.target
        AuthenticationURLProtocol.handler = { $0.respond(400) }
        await viewModel.confirmQRSelection(authManager: auth)
        XCTAssertNotNil(viewModel.errorMessage)
        XCTAssertFalse(viewModel.canConfirmQRSelection)
        XCTAssertEqual(normal.value?.selectedRecoverySessionId, Self.target)
        XCTAssertNotNil(auth.sessionRecovery)
        AuthenticationURLProtocol.handler = { request in XCTFail("Expired grant must not be dispatched again"); request.respond(200) }
        await viewModel.confirmQRSelection(authManager: auth)
    }

    func testRescanningDoesNotConfirmHighlightedCandidate() async throws {
        let auth = await manager()
        let viewModel = SessionRecoveryViewModel()
        AuthenticationURLProtocol.handler = { request in
            XCTAssertNil(PushSetupTestData.body(request.request)["expected_session_id"], "Scanning must not commit an unconfirmed highlighted candidate")
            request.respond(200, data: Self.selection)
        }
        await viewModel.recoverWithQRCode(serverUrl: "https://recovery.test", code: "sb_pair_first", authManager: auth)
        viewModel.selectedSessionId = Self.target
        XCTAssertTrue(viewModel.canConfirmQRSelection)
        await viewModel.recoverWithQRCode(serverUrl: "https://recovery.test", code: "sb_pair_rescan", authManager: auth)
        XCTAssertNil(normal.value?.selectedRecoverySessionId)
        XCTAssertNil(viewModel.selectedSessionId, "Fresh candidates require a new explicit choice and confirmation")
        XCTAssertFalse(viewModel.canConfirmQRSelection)
        XCTAssertNotNil(auth.sessionRecovery)
    }

    func testQRSelectionWithoutScopedGrantFailsClosed() async throws {
        let auth = await manager()
        let payload = try XCTUnwrap(String(data: Self.selection, encoding: .utf8)).replacingOccurrences(of: "\"recovery_token\":\"sb_recover_fixture\",", with: "")
        AuthenticationURLProtocol.handler = { $0.respond(200, data: Data(payload.utf8)) }
        let viewModel = SessionRecoveryViewModel()
        await viewModel.recoverWithQRCode(serverUrl: "https://recovery.test", code: "sb_pair_fixture", authManager: auth)
        XCTAssertNotNil(viewModel.errorMessage)
        XCTAssertTrue(viewModel.candidates.isEmpty)
        XCTAssertNotNil(normal.value)
        XCTAssertNil(normal.value?.selectedRecoverySessionId)
    }

    func testLateQRResultCannotClearReplacementLogin() async throws {
        let auth = await manager()
        let held = AuthenticationRequestLog()
        let sent = expectation(description: "QR verification in flight")
        AuthenticationURLProtocol.handler = { request in held.hold(request); sent.fulfill() }
        let recovery = Task { try await auth.recoverSession(serverUrl: "https://recovery.test", pairingCode: "sb_pair_fixture") }
        await fulfillment(of: [sent], timeout: 3)
        auth.clearAuth()
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "replacement-access", accessExpiresInSecs: 900,
            refreshToken: "replacement-refresh", refreshExpiresInSecs: 3600, tokenType: "Bearer",
            user: MobileUser(id: "bob", username: "bob", role: "member")), origin: "https://replacement.test", installationId: "replacement-installation")
        try XCTUnwrap(held.takeHeld()).respond(200, data: Self.acknowledgement())
        do { _ = try await recovery.value; XCTFail("Late recovery cannot own replacement login") } catch AuthError.staleIdentity { /* Expected generation fence. */ }
        XCTAssertEqual(normal.value?.user.id, "bob")
        XCTAssertEqual(normal.value?.accessToken, "replacement-access")
        XCTAssertTrue(auth.isAuthenticated)
    }

    func testRecoveryAllowsFreshNormalQRCodeLoginAndColdRefresh() async throws {
        let auth = await manager()
        AuthenticationURLProtocol.handler = { $0.respond(200, data: Self.acknowledgement()) }
        _ = try await auth.recoverSession(serverUrl: "https://recovery.test", pairingCode: "sb_pair_cleanup")
        let tokenData = Data("""
        {"data":{"access_token":"fresh-access","access_expires_in_secs":900,"refresh_token":"fresh-refresh",
        "refresh_expires_in_secs":3600,"token_type":"Bearer","mobile_session_id":"33333333-3333-4333-8333-333333333333",
        "revocation_token":"fresh-proof","user":{"id":"alice","username":"alice","role":"member"}}}
        """.utf8)
        AuthenticationURLProtocol.handler = { request in
            XCTAssertEqual(request.request.url?.path, "/api/mobile/auth/pair")
            XCTAssertEqual(PushSetupTestData.body(request.request)["code"] as? String, "sb_pair_new_login")
            request.respond(200, data: tokenData)
        }
        _ = try await AuthViewModel().pair(serverUrl: "https://recovery.test", code: "sb_pair_new_login", authManager: auth, session: session)
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertNil(auth.sessionRecovery)
        XCTAssertEqual(normal.value?.accessToken, "fresh-access")
        AuthenticationURLProtocol.handler = { request in
            XCTAssertEqual(request.request.url?.path, "/api/mobile/auth/refresh")
            XCTAssertEqual(PushSetupTestData.body(request.request)["refresh_token"] as? String, "fresh-refresh")
            request.respond(200, data: tokenData)
        }
        let restarted = AuthManager(revocations: PendingSessionRevocations(storage: journal), authenticationStorage: normal, cleanupSession: session)
        await restarted.initialize()
        XCTAssertTrue(restarted.isAuthenticated)
        XCTAssertNil(restarted.sessionRecovery)
        XCTAssertEqual(restarted.user?.id, "alice")
    }

    func testScannerPayloadRejectsUnrelatedMalformedAndEmptyCodes() {
        for text in ["not-json", #"{"type":"other","server_url":"https://recovery.test","code":"sb_pair_x"}"#,
                     #"{"type":"serverbee_pair","server_url":"https://recovery.test","code":""}"#,
                     #"{"type":"serverbee_pair","server_url":"","code":"sb_pair_x"}"#,
                     #"{"type":"serverbee_pair","server_url":"https://recovery.test","code":123}"#] {
            XCTAssertNil(PairingQRCode.decode(text))
        }
    }
}
