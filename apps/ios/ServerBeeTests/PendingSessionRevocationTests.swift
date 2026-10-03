import Foundation
import XCTest
@testable import ServerBee

@MainActor
private final class MemorySessionRevocations: SessionRevocationStorage {
    var values: [PendingSessionRevocation] = []
    var failure: Error?
    func read() throws -> [PendingSessionRevocation] { if let failure { throw failure }; return values }
    func write(_ records: [PendingSessionRevocation]) throws { if let failure { throw failure }; values = records }
}

@MainActor
private final class MemoryAuthentication: MobileAuthenticationStorage {
    var value: SavedMobileAuthentication?
    var failWrite = false
    var failRead = false
    func read() throws -> SavedMobileAuthentication? {
        if failRead { throw KeychainError.encodingFailed }
        return value
    }
    func write(_ value: SavedMobileAuthentication) throws {
        if failWrite { throw AuthError.secureLogoutNeedsConnection }
        self.value = value
    }
    func delete() throws { value = nil }
}

@MainActor
final class PendingSessionRevocationTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(AuthenticationURLProtocol.self) }
    override func tearDown() async throws {
        AuthenticationURLProtocol.handler = nil
        AuthenticationURLProtocol.cancelPending()
        URLProtocol.unregisterClass(AuthenticationURLProtocol.self)
        AuthManager().clearAuth()
        try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
    }

    private func login(_ auth: AuthManager, name: String = "alice", confirmed: Bool = true) {
        auth.setServerUrl("https://logout.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "access-\(name)", accessExpiresInSecs: 900,
            refreshToken: "refresh-\(name)", refreshExpiresInSecs: 3600, tokenType: "Bearer",
            user: MobileUser(id: name, username: name, role: "member"),
            revocationToken: confirmed ? "deletion-\(name)" : nil,
            mobileSessionId: confirmed ? (name == "alice" ? "11111111-1111-4111-8111-111111111111" : "22222222-2222-4222-8222-222222222222") : nil))
    }

    func testOfflineLogoutRestartsSignedOutAndReplaysOnlyOriginalDeletionProof() async throws {
        let journal = MemorySessionRevocations()
        let normal = MemoryAuthentication()
        let auth = AuthManager(revocations: PendingSessionRevocations(storage: journal), authenticationStorage: normal)
        login(auth)
        AuthenticationURLProtocol.handler = { $0.loseResponse() }
        let context = try XCTUnwrap(auth.captureContext())
        await auth.endSession(context: context)
        XCTAssertFalse(auth.isAuthenticated)
        XCTAssertNil(normal.value)
        XCTAssertEqual(journal.values.count, 1)
        let encoded = String(decoding: try JSONEncoder().encode(journal.values), as: UTF8.self)
        XCTAssertFalse(encoded.contains("access-alice"))
        XCTAssertFalse(encoded.contains("refresh-alice"))
        AuthenticationURLProtocol.handler = { request in
            XCTAssertEqual(request.request.url?.absoluteString, "https://logout.test/api/mobile/auth/revoke")
            XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
            let body = PushSetupTestData.body(request.request)
            XCTAssertEqual(body["revocation_token"] as? String, "deletion-alice")
            XCTAssertEqual(body["expected_session_id"] as? String, "11111111-1111-4111-8111-111111111111")
            request.respond(200)
        }
        let restarted = AuthManager(revocations: PendingSessionRevocations(storage: journal), authenticationStorage: normal)
        await restarted.initialize()
        XCTAssertFalse(restarted.isAuthenticated)
        XCTAssertTrue(journal.values.isEmpty)
    }

    func testLegacyOfflineLogoutPreservesNormalCredentialsAndReportsMigrationBlocker() async throws {
        let journal = MemorySessionRevocations()
        let normal = MemoryAuthentication()
        let auth = AuthManager(revocations: PendingSessionRevocations(storage: journal), authenticationStorage: normal)
        login(auth, confirmed: false)
        AuthenticationURLProtocol.handler = { $0.loseResponse() }
        await auth.endSession(context: try XCTUnwrap(auth.captureContext()))
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertEqual(normal.value?.refreshToken, "refresh-alice")
        XCTAssertNotNil(auth.recoveryError)
        XCTAssertTrue(journal.values.isEmpty)
    }

    func testJournalFailureCannotDiscardTheOnlyConfirmedProof() async throws {
        let journal = MemorySessionRevocations()
        let normal = MemoryAuthentication()
        let auth = AuthManager(revocations: PendingSessionRevocations(storage: journal), authenticationStorage: normal)
        login(auth)
        journal.failure = AuthError.cleanupCapacity
        AuthenticationURLProtocol.handler = { request in XCTFail("No destructive network step before durable save"); request.respond(500) }
        await auth.endSession(context: try XCTUnwrap(auth.captureContext()))
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertEqual(normal.value?.confirmedDeletionProof, "deletion-alice")
        XCTAssertNotNil(auth.recoveryError)
    }

    func testAtomicFreshLoginWriteFailureNeverPublishesNewIdentity() {
        let journal = MemorySessionRevocations()
        let normal = MemoryAuthentication()
        normal.failWrite = true
        let auth = AuthManager(revocations: PendingSessionRevocations(storage: journal), authenticationStorage: normal)
        login(auth)
        XCTAssertFalse(auth.isAuthenticated)
        XCTAssertNil(normal.value)
        XCTAssertNil(auth.captureContext())
        XCTAssertNotNil(auth.recoveryError)
    }

    func testUnconfirmedBootstrapCausesNoSettingsWriteOrRelayRegistration() async throws {
        let journal = MemorySessionRevocations()
        let normal = MemoryAuthentication()
        let auth = AuthManager(revocations: PendingSessionRevocations(storage: journal), authenticationStorage: normal)
        login(auth, confirmed: false)
        let log = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            if request.request.url?.path == "/api/mobile/push/settings", request.request.httpMethod == "GET" {
                request.respond(200, data: PushSetupTestData.response())
            } else {
                XCTAssertEqual(request.request.url?.path, "/api/mobile/auth/refresh")
                let proposal = PushSetupTestData.body(request.request)["revocation_proof"] as? String
                XCTAssertTrue(proposal?.hasPrefix("sb-revoke-v1.") == true)
                request.loseResponse()
            }
        }
        let relay = TestPushRelay()
        let manager = PushNotificationManager(system: TestPushSystem(), relay: relay, storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        await manager.savePreferences(PushPreferences(enabled: false))
        manager.didRegisterForRemoteNotifications(deviceToken: Data([1]))
        await manager.waitForPendingRegistrations()
        XCTAssertEqual(relay.attempts, 0)
        XCTAssertFalse(log.snapshot().contains { $0.httpMethod == "PUT" || $0.url?.path == "/api/mobile/push/verified-register" })
        XCTAssertEqual(normal.value?.refreshToken, "refresh-alice")
        XCTAssertNotNil(normal.value?.proposedDeletionProof)
    }

    func testRejectedOrRedirectedReplayRetainsExactRecordAndCapacityNeverEvicts() async throws {
        let journal = MemorySessionRevocations()
        let store = PendingSessionRevocations(storage: journal)
        for index in 0..<PendingSessionRevocations.capacity {
            try store.enqueue(PendingSessionRevocation(id: UUID(), serverUrl: "https://logout.test", userId: "alice",
                installationId: "installation", mobileSessionId: UUID().uuidString, proof: "deletion-\(index)"))
        }
        XCTAssertThrowsError(try store.requireLoginCapacity())
        let before = journal.values
        AuthenticationURLProtocol.handler = { $0.respond(307) }
        let auth = AuthManager(revocations: store, authenticationStorage: MemoryAuthentication())
        await auth.retryPendingRevocations()
        XCTAssertEqual(journal.values, before)
        AuthenticationURLProtocol.handler = { $0.respond(401) }
        await auth.retryPendingRevocations()
        XCTAssertEqual(journal.values, before)
    }
    func testUnreadableSnapshotBlocksLoginWithoutOverwritingOrSendingCredentials() async {
        let normal = MemoryAuthentication()
        normal.failRead = true
        let auth = AuthManager(revocations: PendingSessionRevocations(storage: MemorySessionRevocations()), authenticationStorage: normal)
        await auth.initialize()
        XCTAssertFalse(auth.isAuthenticated)
        AuthenticationURLProtocol.handler = { request in XCTFail("A failed auth read must block login HTTP"); request.respond(200) }
        let viewModel = AuthViewModel()
        viewModel.serverUrlInput = "https://logout.test"
        viewModel.username = "alice"
        viewModel.password = "fixture"
        await viewModel.login(authManager: auth)
        XCTAssertFalse(auth.isAuthenticated)
        XCTAssertFalse(viewModel.errorMessage.isEmpty)
        XCTAssertNil(normal.value)
    }

    func testRefreshWriteFailurePreservesTheOriginalAtomicIdentityAndProof() async throws {
        let normal = MemoryAuthentication()
        let journal = MemorySessionRevocations()
        let auth = AuthManager(revocations: PendingSessionRevocations(storage: journal), authenticationStorage: normal)
        login(auth)
        normal.failWrite = true
        let response = Data("""
        {"data":{"access_token":"new-access","access_expires_in_secs":900,"refresh_token":"new-refresh",
        "refresh_expires_in_secs":3600,"token_type":"Bearer","user":{"id":"alice","username":"alice","role":"member"},
        "mobile_session_id":"11111111-1111-4111-8111-111111111111"}}
        """.utf8)
        AuthenticationURLProtocol.handler = { $0.respond(200, data: response) }
        do { _ = try await auth.refreshAccessToken(); XCTFail("Expected atomic persistence failure") } catch { }
        XCTAssertEqual(auth.getAccessToken(), "access-alice")
        XCTAssertEqual(normal.value?.refreshToken, "refresh-alice")
        XCTAssertEqual(normal.value?.confirmedDeletionProof, "deletion-alice")
        XCTAssertEqual(normal.value?.mobileSessionId, "11111111-1111-4111-8111-111111111111")
    }

    func testHeldOldReplayNeverUsesReplacementCredentialsOrClearsItsRecord() async throws {
        let normal = MemoryAuthentication()
        let journal = MemorySessionRevocations()
        let auth = AuthManager(revocations: PendingSessionRevocations(storage: journal), authenticationStorage: normal)
        login(auth)
        let started = expectation(description: "original proof held in flight")
        let log = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            log.hold(request)
            started.fulfill()
        }
        let context = try XCTUnwrap(auth.captureContext())
        let logout = Task { await auth.endSession(context: context) }
        await fulfillment(of: [started], timeout: 3)
        login(auth, name: "bob")
        try XCTUnwrap(log.takeHeld()).respond(200)
        await logout.value
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertEqual(auth.user?.id, "bob")
        XCTAssertEqual(normal.value?.confirmedDeletionProof, "deletion-bob")
        XCTAssertEqual(log.snapshot().count, 1)
        XCTAssertNil(log.snapshot().first?.value(forHTTPHeaderField: "Authorization"))
        XCTAssertEqual(PushSetupTestData.body(try XCTUnwrap(log.snapshot().first))["expected_session_id"] as? String, "11111111-1111-4111-8111-111111111111")
        XCTAssertTrue(journal.values.isEmpty)
    }

}

extension PendingSessionRevocationTests {
    func testBootstrapPersistsProposalBeforeRefreshAndConfirmsBeforeFirstSettingsPut() async throws {
        let normal = MemoryAuthentication()
        let auth = AuthManager(revocations: PendingSessionRevocations(storage: MemorySessionRevocations()), authenticationStorage: normal)
        login(auth, confirmed: false)
        let log = AuthenticationRequestLog()
        let entered = expectation(description: "bootstrap request held before its response")
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            if request.request.url?.path == "/api/mobile/auth/refresh" {
                log.hold(request)
                entered.fulfill()
            } else {
                XCTAssertEqual(request.request.httpMethod, "PUT")
                XCTAssertEqual(request.request.value(forHTTPHeaderField: "Authorization"), "Bearer rotated")
                request.respond(200, data: PushSetupTestData.response())
            }
        }
        let context = try XCTUnwrap(auth.captureContext())
        let saving = Task {
            let _: PushSetup = try await APIClient(authManager: auth).send("/api/mobile/push/settings", method: "PUT",
                body: PushPreferencesRequest(expectedRevision: 0, preferences: PushPreferences(enabled: false)), context: context)
        }
        defer { saving.cancel() }
        await fulfillment(of: [entered], timeout: 3)
        let held = try XCTUnwrap(log.takeHeld())
        let proposal = try XCTUnwrap(PushSetupTestData.body(held.request)["revocation_proof"] as? String)
        XCTAssertEqual(proposal, normal.value?.proposedDeletionProof)
        XCTAssertNil(normal.value?.confirmedDeletionProof)
        let response = MobileTokenResponse(accessToken: "rotated", accessExpiresInSecs: 900,
            refreshToken: "rotated-refresh", refreshExpiresInSecs: 3600, tokenType: "Bearer",
            user: MobileUser(id: "alice", username: "alice", role: "member"),
            revocationToken: proposal, mobileSessionId: "11111111-1111-4111-8111-111111111111")
        var data = Data(#"{"data":"#.utf8)
        data.append(try JSONEncoder().encode(response))
        data.append(Data("}".utf8))
        held.respond(200, data: data)
        try await saving.value
        XCTAssertEqual(log.snapshot().compactMap { $0.url?.path }, ["/api/mobile/auth/refresh", "/api/mobile/push/settings"])
        XCTAssertEqual(normal.value?.mobileSessionId, "11111111-1111-4111-8111-111111111111")
        XCTAssertNotNil(normal.value?.confirmedDeletionProof)
        XCTAssertNil(normal.value?.proposedDeletionProof)
        XCTAssertEqual(auth.captureContext()?.pushScope, context.pushScope)
    }

    func testSavedCleanupIgnoresReplacementCookiesAndAcceptsOnlyScopedAcknowledgements() async throws {
        let cookie = try XCTUnwrap(HTTPCookie(properties: [.domain: "logout.test", .path: "/", .name: "session", .value: "replacement-cookie"]))
        HTTPCookieStorage.shared.setCookie(cookie)
        defer { HTTPCookieStorage.shared.deleteCookie(cookie) }
        AuthenticationURLProtocol.handler = { request in
            XCTAssertNil(request.request.value(forHTTPHeaderField: "Cookie"))
            XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
            XCTAssertFalse(request.request.httpShouldHandleCookies)
            request.respond(200, data: Data(#"{"data":"already_absent"}"#.utf8))
        }
        let record = PendingSessionRevocation(id: UUID(), serverUrl: "https://logout.test", userId: "alice",
            installationId: "installation", mobileSessionId: UUID().uuidString, proof: "deletion-alice")
        try await APIClient.revokeSavedSession(record)
    }

    func testOldLogoutCompletionCannotEraseAReplacementLoginFailure() async throws {
        let normal = MemoryAuthentication()
        let journal = MemorySessionRevocations()
        let auth = AuthManager(revocations: PendingSessionRevocations(storage: journal), authenticationStorage: normal)
        login(auth)
        let started = expectation(description: "old logout held")
        let log = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in log.hold(request); started.fulfill() }
        let context = try XCTUnwrap(auth.captureContext())
        let logout = Task { await auth.endSession(context: context) }
        await fulfillment(of: [started], timeout: 3)
        normal.failWrite = true
        login(auth, name: "bob")
        let failure = try XCTUnwrap(auth.recoveryError)
        try XCTUnwrap(log.takeHeld()).respond(200)
        await logout.value
        XCTAssertEqual(auth.recoveryError, failure)
        XCTAssertFalse(auth.isAuthenticated)
    }

    func testMalformedBootstrapIdentityCannotCreatePushOwnership() async throws {
        let normal = MemoryAuthentication()
        let auth = AuthManager(revocations: PendingSessionRevocations(storage: MemorySessionRevocations()), authenticationStorage: normal)
        login(auth, confirmed: false)
        AuthenticationURLProtocol.handler = { request in
            XCTAssertEqual(request.request.url?.path, "/api/mobile/auth/refresh")
            let proposal = PushSetupTestData.body(request.request)["revocation_proof"] as? String
            let response = MobileTokenResponse(accessToken: "new", accessExpiresInSecs: 900,
                refreshToken: "new-refresh", refreshExpiresInSecs: 3600, tokenType: "Bearer",
                user: MobileUser(id: "alice", username: "alice", role: "member"),
                revocationToken: proposal, mobileSessionId: "not-a-session-uuid")
            var data = Data(#"{"data":"#.utf8)
            data.append((try? JSONEncoder().encode(response)) ?? Data())
            data.append(Data("}".utf8))
            request.respond(200, data: data)
        }
        do {
            let _: PushSetup = try await APIClient(authManager: auth).send("/api/mobile/push/settings", method: "PUT",
                body: PushPreferencesRequest(expectedRevision: 0, preferences: PushPreferences(enabled: false)),
                context: try XCTUnwrap(auth.captureContext()))
            XCTFail("Malformed session identity cannot authorize a settings mutation")
        } catch AuthError.invalidSessionResponse { } catch { XCTFail("Unexpected error: \(error)") }
        XCTAssertNil(normal.value?.confirmedDeletionProof)
        XCTAssertNil(normal.value?.mobileSessionId)
        XCTAssertEqual(normal.value?.refreshToken, "refresh-alice")
    }

    func testLegacyMigrationBootstrapAndFreshAuthRestartPreservePushArtifactsAndTestIdentity() async throws {
        for legacyProof in [nil, "legacy-scope-proof"] as [String?] {
            let installation = try InstallationID.getOrCreateThrowing()
            let user = MobileUser(id: "alice", username: "alice", role: "member")
            try KeychainService.saveString("https://logout.test", for: KeychainService.serverUrlKey)
            try KeychainService.saveString("legacy-access", for: KeychainService.accessTokenKey)
            try KeychainService.saveString("legacy-refresh", for: KeychainService.refreshTokenKey)
            if let legacyProof { try KeychainService.saveString(legacyProof, for: KeychainService.revocationTokenKey) }
            else { KeychainService.delete(for: KeychainService.revocationTokenKey) }
            try KeychainService.saveCodable(user, for: KeychainService.userKey)
            let old = MobileAuthenticationContext(serverUrl: "https://logout.test", userId: user.id, installationId: installation,
                generation: UUID(), accessToken: "legacy-access", revocationToken: legacyProof ?? "legacy-refresh", refreshToken: "legacy-refresh")
            let storage = MemoryPushSetupStorage()
            let content = PushContentKey(keyId: UUID().uuidString, key: Data(repeating: 1, count: 32).base64EncodedString(),
                deploymentId: old.serverUrl, userId: user.id, installationId: installation, scope: old.pushScope)
            let contentBytes = try JSONEncoder().encode(content)
            try storage.save(contentBytes, key: PushContentKey.storageKey)
            let grantKey = "serverbee_pending_push_" + old.pushScope
            let grantBytes = Data("existing-pending-grant-fixture".utf8)
            try storage.save(grantBytes, key: grantKey)
            let event = UUID().uuidString.lowercased()
            try storage.save(JSONEncoder().encode(SavedTestPush(scope: old.pushScope,
                request: TestPushRequest(eventId: event, expectedRevision: 2), admission: .admitted)), key: "serverbee_pending_push_test")
            let log = AuthenticationRequestLog()
            AuthenticationURLProtocol.handler = { request in
                _ = log.append(request.request)
                if request.request.url?.path == "/api/mobile/auth/refresh" {
                    let proposal = PushSetupTestData.body(request.request)["revocation_proof"] as? String
                    let tokens = MobileTokenResponse(accessToken: "rotated", accessExpiresInSecs: 900,
                        refreshToken: "rotated-refresh", refreshExpiresInSecs: 3600, tokenType: "Bearer", user: user,
                        revocationToken: proposal, mobileSessionId: "11111111-1111-4111-8111-111111111111")
                    var data = Data(#"{"data":"#.utf8)
                    data.append((try? JSONEncoder().encode(tokens)) ?? Data())
                    data.append(Data("}".utf8))
                    request.respond(200, data: data)
                } else {
                    XCTAssertEqual(request.request.httpMethod, "GET", "An admitted test cannot be posted again after migration")
                    XCTAssertEqual(request.request.url?.path, "/api/mobile/push/test/" + event)
                    request.respond(200, data: Data("{\"data\":{\"event_id\":\"\(event)\",\"outcome\":\"pending\",\"reason\":\"Fixture\"}}".utf8))
                }
            }
            let normal = MemoryAuthentication()
            let journal = PendingSessionRevocations(storage: MemorySessionRevocations())
            let first = AuthManager(revocations: journal, authenticationStorage: normal)
            await first.initialize()
            XCTAssertTrue(first.isAuthenticated)
            XCTAssertEqual(first.captureContext()?.pushScope, old.pushScope)
            XCTAssertNotNil(normal.value?.confirmedDeletionProof)
            XCTAssertEqual(normal.value?.revocationToken, legacyProof ?? "legacy-refresh")
            let restarted = AuthManager(revocations: journal, authenticationStorage: normal)
            await restarted.initialize()
            XCTAssertEqual(restarted.authenticationGeneration, first.authenticationGeneration)
            XCTAssertEqual(restarted.captureContext()?.pushScope, old.pushScope)
            let manager = PushNotificationManager(system: TestPushSystem(), relay: TestPushRelay(), storage: storage)
            let api = APIClient(authManager: restarted)
            manager.configure(apiClient: api)
            XCTAssertEqual(storage.load(PushContentKey.storageKey), contentBytes)
            XCTAssertEqual(storage.load(grantKey), grantBytes)
            let delivery = PushTestDelivery(storage: storage)
            delivery.configure(apiClient: api)
            await delivery.refresh(setup: nil)
            XCTAssertEqual(delivery.result?.eventId, event)
            XCTAssertEqual(log.snapshot().filter { $0.url?.path.hasPrefix("/api/mobile/push/test") == true }.count, 1)
        }
    }

    func testNoContextLogoutCannotClearLoginCreatedWhileClosingOrUnregistering() async {
        for heldStep in ["close", "unregister"] {
            let normal = MemoryAuthentication()
            let auth = AuthManager(revocations: PendingSessionRevocations(storage: MemorySessionRevocations()), authenticationStorage: normal)
            let started = expectation(description: "nil-context logout held at " + heldStep)
            let gate = AsyncStream<Void>.makeStream()
            let logout = Task {
                await SettingsViewModel().logout(authManager: auth, unregisterPush: { context in
                    XCTAssertNil(context)
                    if heldStep == "unregister" {
                        started.fulfill()
                        for await _ in gate.stream { break }
                    }
                }, closeWebSocket: {
                    if heldStep == "close" {
                        started.fulfill()
                        for await _ in gate.stream { break }
                    }
                })
            }
            await fulfillment(of: [started], timeout: 3)
            login(auth, name: "bob")
            gate.continuation.yield(())
            gate.continuation.finish()
            await logout.value
            XCTAssertTrue(auth.isAuthenticated)
            XCTAssertEqual(auth.user?.id, "bob")
            XCTAssertEqual(normal.value?.refreshToken, "refresh-bob")
        }
    }

}
