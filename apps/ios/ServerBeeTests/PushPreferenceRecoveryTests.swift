import Foundation
import XCTest
@testable import ServerBee

/// Simulates Server replies only; preference recovery uses the real manager and API client.
final class PreferenceRecoveryHTTP: @unchecked Sendable {
    private let lock = NSLock()
    private var preferences = PushPreferences(enabled: true, alerts: true, security: false, taskFailure: true, taskSuccess: false)
    private var revision: Int64 = 2
    private var registered = true
    private var securityAllowed = true
    private var tasksAllowed = true
    private var saveStatus = 200
    private var registerStatus = 200
    private var loseSaveReply = false
    private var attempts: [PushPreferences] = []
    private var nextRead: XCTestExpectation?
    private var held: (request: PreferenceRecoveryURLProtocol, data: Data)?
    private var registration: XCTestExpectation?
    private var registrationRequests: [URLRequest] = []

    init(security: Bool = false) { preferences.security = security }
    func demote() { lock.lock(); defer { lock.unlock() }; securityAllowed = false; tasksAllowed = false }

    func revokeTaskAccess() { demote() }

    func rejectSave() { lock.lock(); defer { lock.unlock() }; saveStatus = 503 }
    func acceptSave() { lock.lock(); defer { lock.unlock() }; saveStatus = 200 }
    func loseCommittedSaveReply() { lock.lock(); defer { lock.unlock() }; loseSaveReply = true }
    func setRegistrationStatus(_ status: Int) { lock.lock(); defer { lock.unlock() }; registerStatus = status }
    func savedPreferences() -> [PushPreferences] { lock.lock(); defer { lock.unlock() }; return attempts }
    func registrationAttempts() -> [URLRequest] { lock.lock(); defer { lock.unlock() }; return registrationRequests }
    func observeRegistration(_ entered: XCTestExpectation) { lock.lock(); defer { lock.unlock() }; registration = entered }
    func holdNextRead(_ entered: XCTestExpectation) { lock.lock(); defer { lock.unlock() }; nextRead = entered }
    func release() {
        lock.lock()
        let reply = held
        held = nil
        lock.unlock()
        if let reply { reply.request.respond(200, data: reply.data) }
    }
    func cancel() {
        lock.lock()
        let request = held?.request
        held = nil
        nextRead = nil
        registration = nil
        lock.unlock()
        request?.fail(URLError(.cancelled))
    }

    func handle(_ request: PreferenceRecoveryURLProtocol) {
        lock.lock()
        var status = 200
        var lost = false
        var observed: XCTestExpectation?
        do {
            let path = request.request.url?.path
            if path == "/api/mobile/push/settings", request.request.httpMethod == "PUT" {
                let body = PushSetupTestData.body(request.request)
                guard let expected = body["expected_revision"] as? NSNumber, let draft = body["preferences"] else {
                    throw URLError(.badServerResponse)
                }
                let encoded = try JSONSerialization.data(withJSONObject: draft)
                let submitted = try JSONDecoder.snakeCase.decode(PushPreferences.self, from: encoded)
                attempts.append(submitted)
                status = expected.int64Value == revision ? saveStatus : 409
                let forbiddenSecurity = submitted.security && !securityAllowed
                let forbiddenTasks = (submitted.taskFailure || submitted.taskSuccess) && !tasksAllowed
                if status == 200, forbiddenSecurity || forbiddenTasks { status = 403 }
                if status == 200 {
                    preferences = submitted
                    revision = expected.int64Value + 1
                    if !preferences.enabled { registered = false }
                    lost = loseSaveReply
                    loseSaveReply = false
                }
            } else if path == "/api/mobile/push/verified-register", request.request.httpMethod == "POST" {
                registrationRequests.append(request.request)
                observed = registration
                registration = nil
                guard let expected = PushSetupTestData.body(request.request)["expected_revision"] as? NSNumber else {
                    throw URLError(.badServerResponse)
                }
                status = expected.int64Value == revision ? registerStatus : 409
                if status == 200 { revision = expected.int64Value + 1; registered = true }
            } else if path != "/api/mobile/push/settings" || request.request.httpMethod != "GET" {
                throw URLError(.unsupportedURL)
            }
            let data = status == 200
                ? PushSetupTestData.response(registered: registered, revision: revision, preferences: preferences, securityAllowed: securityAllowed, tasksAllowed: tasksAllowed)
                : Data(#"{"error":{"message":"Fixture write rejected"}}"#.utf8)
            if request.request.httpMethod == "GET", let entered = nextRead {
                held = (request, data)
                nextRead = nil
                lock.unlock()
                entered.fulfill()
                return
            }
            lock.unlock()
            observed?.fulfill()
            if lost { request.fail(URLError(.networkConnectionLost)) } else { request.respond(status, data: data) }
        } catch {
            lock.unlock()
            observed?.fulfill()
            request.fail(error)
        }
    }
}

final class PreferenceRecoveryURLProtocol: URLProtocol {
    nonisolated(unsafe) static var fixture: PreferenceRecoveryHTTP?
    private static let pending = PendingURLProtocolRequests()
    override static func canInit(with request: URLRequest) -> Bool { true }
    override static func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        Self.pending.begin(self)
        if let fixture = Self.fixture { fixture.handle(self) } else { fail(URLError(.cancelled)) }
    }
    override func stopLoading() { _ = Self.pending.finish(self) }
    static func cancelPending() { pending.cancelAll() }
    func respond(_ status: Int, data: Data) {
        guard let url = request.url,
              let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: nil, headerFields: nil) else {
            fail(URLError(.badServerResponse))
            return
        }
        guard Self.pending.finish(self) else { return }
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }
    func fail(_ error: Error) {
        guard Self.pending.finish(self) else { return }
        client?.urlProtocol(self, didFailWithError: error)
    }
}

@MainActor
final class PushPreferenceRecoveryTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(PreferenceRecoveryURLProtocol.self) }
    override func tearDown() async throws {
        PreferenceRecoveryURLProtocol.fixture?.cancel()
        PreferenceRecoveryURLProtocol.fixture = nil
        PreferenceRecoveryURLProtocol.cancelPending()
        URLProtocol.unregisterClass(PreferenceRecoveryURLProtocol.self)
        AuthManager().clearAuth()
        try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
    }
    func login(_ auth: AuthManager, user: String = "alice") {
        auth.setServerUrl("https://\(user).test")
        auth.handleLoginResponse(MobileTokenResponse(
            accessToken: "access-\(UUID().uuidString)", accessExpiresInSecs: 900,
            refreshToken: "refresh-\(UUID().uuidString)", refreshExpiresInSecs: 3600,
            tokenType: "Bearer", user: MobileUser(id: user, username: user, role: "member"),
            revocationToken: "proof-\(UUID().uuidString)",
            mobileSessionId: UUID().uuidString))
    }
    func manager(_ auth: AuthManager, system: TestPushSystem, relay: TestPushRelay? = nil) -> PushNotificationManager {
        let manager = PushNotificationManager(system: system, relay: relay ?? TestPushRelay(), storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        return manager
    }

    func testRejectedSaveRemainsVisibleAfterOldReadsAndRetryUntilSuccessfulSave() async throws {
        let http = PreferenceRecoveryHTTP()
        PreferenceRecoveryURLProtocol.fixture = http
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PreferenceRecoveryURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        system.status = .denied
        let manager = manager(auth, system: system)
        await manager.reconcile()
        let original = try XCTUnwrap(manager.confirmed?.preferences)
        var desired = original
        desired.alerts = false
        http.rejectSave()
        await manager.savePreferences(desired)
        let failure = try XCTUnwrap(manager.errorMessage)
        for _ in 0..<2 {
            await manager.reconcile() // Launch, foreground and connectivity use the same owned read.
            XCTAssertEqual(manager.errorMessage, failure)
            XCTAssertEqual(manager.unconfirmedPreferences, desired)
            XCTAssertEqual(manager.confirmed?.preferences, original)
            XCTAssertEqual(manager.confirmed?.revision, 2)
            XCTAssertEqual(manager.confirmed?.registered, true)
        }
        await manager.retry()
        XCTAssertEqual(manager.errorMessage, failure)
        XCTAssertEqual(manager.unconfirmedPreferences, desired)
        http.acceptSave()
        await manager.savePreferences(desired)
        XCTAssertNil(manager.errorMessage)
        XCTAssertNil(manager.unconfirmedPreferences)
        XCTAssertEqual(manager.confirmed?.preferences, desired)
        XCTAssertEqual(manager.confirmed?.revision, 3)
        XCTAssertEqual(http.savedPreferences(), [desired, desired])
        XCTAssertEqual(system.permissionRequests, 0)
        await manager.waitForPendingRegistrations()
    }

    func testLostSaveResponseClearsOnlyAfterNewerReadConfirmsPermittedPreferences() async throws {
        for currentAdmin in [true, false] {
            for enabled in [true, false] {
                let http = PreferenceRecoveryHTTP()
                if !currentAdmin { http.demote() }
                PreferenceRecoveryURLProtocol.fixture = http
                let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PreferenceRecoveryURLProtocol.self]))
                auth.clearAuth()
                login(auth)
                let system = TestPushSystem()
                system.status = .denied
                let relay = TestPushRelay()
                let manager = manager(auth, system: system, relay: relay)
                await manager.reconcile()
                // Category visibility follows the actual Server role, not cached login metadata.
                XCTAssertEqual(auth.user?.role, "member")
                XCTAssertEqual(manager.confirmed?.securityAllowed, currentAdmin)
                XCTAssertEqual(manager.confirmed?.tasksAllowed, currentAdmin)
                let original = try XCTUnwrap(manager.confirmed?.preferences)
                var draft = original
                draft.enabled = enabled
                draft.alerts = false
                draft.security = true
                draft.taskFailure = true
                draft.taskSuccess = true
                var permitted = draft
                if !currentAdmin {
                    permitted.security = false
                    permitted.taskFailure = false
                    permitted.taskSuccess = false
                }
                http.loseCommittedSaveReply()
                await manager.savePreferences(draft)
                XCTAssertNotNil(manager.errorMessage)
                XCTAssertEqual(manager.unconfirmedPreferences, permitted)
                XCTAssertEqual(manager.confirmed?.preferences, original)
                XCTAssertEqual(manager.confirmed?.revision, 2)
                XCTAssertEqual(manager.confirmed?.registered, true)
                XCTAssertEqual(http.savedPreferences(), [permitted])
                XCTAssertEqual(system.permissionRequests, 0)
                XCTAssertEqual(relay.attempts, 0)
                await manager.reconcile()
                XCTAssertNil(manager.errorMessage)
                XCTAssertNil(manager.unconfirmedPreferences)
                XCTAssertEqual(manager.confirmed?.preferences, permitted)
                XCTAssertEqual(manager.confirmed?.revision, 3)
                XCTAssertEqual(manager.confirmed?.securityAllowed, currentAdmin)
                XCTAssertEqual(manager.confirmed?.tasksAllowed, currentAdmin)
                XCTAssertEqual(manager.confirmed?.registered, enabled)
                XCTAssertEqual(http.savedPreferences(), [permitted], "Recovery reads must not resubmit the committed intent")
                await manager.waitForPendingRegistrations()
                XCTAssertEqual(system.permissionRequests, 0)
                XCTAssertEqual(relay.attempts, 0)
                XCTAssertEqual(relay.revocations, 0)
                auth.clearAuth()
            }
        }
    }

    func testEqualPreferencesAtOriginalRevisionCannotResolveFailedSave() async throws {
        let http = PreferenceRecoveryHTTP()
        PreferenceRecoveryURLProtocol.fixture = http
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PreferenceRecoveryURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        system.status = .denied
        let manager = manager(auth, system: system)
        await manager.reconcile()
        let unchanged = try XCTUnwrap(manager.confirmed?.preferences)
        http.rejectSave()
        await manager.savePreferences(unchanged)
        let failure = try XCTUnwrap(manager.errorMessage)
        await manager.reconcile()
        XCTAssertEqual(manager.confirmed?.preferences, unchanged)
        XCTAssertEqual(manager.confirmed?.revision, 2)
        XCTAssertEqual(manager.errorMessage, failure)
        XCTAssertEqual(manager.unconfirmedPreferences, unchanged)
    }
}

extension PushPreferenceRecoveryTests {
    func testDemotionCannotConfirmForbiddenIntentAtNewerRegistrationRevision() async throws {
        let http = PreferenceRecoveryHTTP(security: true)
        PreferenceRecoveryURLProtocol.fixture = http
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PreferenceRecoveryURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        let manager = manager(auth, system: system)
        await manager.reconcile()
        let stale = try XCTUnwrap(manager.confirmed?.preferences)
        XCTAssertTrue(stale.security)
        http.demote()
        await manager.savePreferences(stale)
        let failure = try XCTUnwrap(manager.errorMessage)
        let registered = expectation(description: "registration advances revision after role demotion")
        http.observeRegistration(registered)
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 10, count: 32))
        await fulfillment(of: [registered], timeout: 3)
        await manager.waitForPendingRegistrations()
        await manager.reconcile()
        XCTAssertEqual(manager.confirmed?.revision, 3)
        XCTAssertEqual(manager.confirmed?.preferences, stale)
        XCTAssertEqual(manager.confirmed?.securityAllowed, false)
        XCTAssertEqual(manager.errorMessage, failure)
        XCTAssertEqual(manager.unconfirmedPreferences, stale)
        await manager.savePreferences(stale) // The now-confirmed role clears security in the actual request.
        XCTAssertNil(manager.errorMessage)
        XCTAssertNil(manager.unconfirmedPreferences)
        XCTAssertEqual(manager.confirmed?.preferences.security, false)
        XCTAssertEqual(manager.confirmed?.preferences.taskFailure, false)
        XCTAssertEqual(manager.confirmed?.preferences.taskSuccess, false)
        XCTAssertEqual(manager.confirmed?.tasksAllowed, false)
        XCTAssertEqual(manager.confirmed?.revision, 4)
        XCTAssertEqual(system.permissionRequests, 0)
        await manager.waitForPendingRegistrations()
    }

    func testOldConfirmingReadCannotAffectReplacementLoginOrItsFailedSave() async throws {
        let original = PreferenceRecoveryHTTP()
        let replacement = PreferenceRecoveryHTTP()
        defer { original.cancel(); replacement.cancel() }
        PreferenceRecoveryURLProtocol.fixture = original
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PreferenceRecoveryURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        system.status = .denied
        let manager = manager(auth, system: system)
        await manager.reconcile()
        var desired = try XCTUnwrap(manager.confirmed?.preferences)
        desired.alerts = false
        original.loseCommittedSaveReply()
        await manager.savePreferences(desired)
        XCTAssertNotNil(manager.errorMessage)
        let entered = expectation(description: "old login confirming GET held")
        original.holdNextRead(entered)
        let oldRead = Task { @MainActor in await manager.reconcile() }
        await fulfillment(of: [entered], timeout: 3)
        auth.clearAuth()
        login(auth, user: "bob")
        manager.configure(apiClient: APIClient(authManager: auth))
        XCTAssertNil(manager.errorMessage)
        XCTAssertNil(manager.unconfirmedPreferences)
        PreferenceRecoveryURLProtocol.fixture = replacement
        await manager.reconcile()
        var replacementDesired = try XCTUnwrap(manager.confirmed?.preferences)
        replacementDesired.taskSuccess = true
        replacement.rejectSave()
        await manager.savePreferences(replacementDesired)
        let failure = try XCTUnwrap(manager.errorMessage)
        original.release()
        await oldRead.value
        XCTAssertEqual(manager.errorMessage, failure)
        XCTAssertEqual(manager.unconfirmedPreferences, replacementDesired)
        XCTAssertEqual(manager.confirmed?.revision, 2)
        XCTAssertEqual(manager.confirmed?.preferences.taskSuccess, false)
        XCTAssertEqual(auth.user?.id, "bob")
        XCTAssertEqual(auth.serverUrl, "https://bob.test")
        XCTAssertTrue(auth.isAuthenticated)
        await manager.waitForPendingRegistrations()
    }

    func testRegistrationRecoveryDoesNotHideUnsavedPreferences() async throws {
        let http = PreferenceRecoveryHTTP()
        PreferenceRecoveryURLProtocol.fixture = http
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PreferenceRecoveryURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        let relay = TestPushRelay()
        let manager = manager(auth, system: system, relay: relay)
        await manager.reconcile()
        let original = try XCTUnwrap(manager.confirmed?.preferences)
        var desired = original
        desired.alerts = false
        http.rejectSave()
        await manager.savePreferences(desired)
        let failure = try XCTUnwrap(manager.errorMessage)
        http.setRegistrationStatus(503)
        let rejected = expectation(description: "unrelated registration rejected")
        http.observeRegistration(rejected)
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 10, count: 32))
        await fulfillment(of: [rejected], timeout: 3)
        await manager.waitForPendingRegistrations()
        XCTAssertEqual(manager.errorMessage, failure)
        XCTAssertEqual(manager.unconfirmedPreferences, desired)
        XCTAssertEqual(manager.confirmed?.registered, false)
        http.setRegistrationStatus(200)
        let registered = expectation(description: "unrelated registration recovered")
        http.observeRegistration(registered)
        await manager.retry()
        await fulfillment(of: [registered], timeout: 3)
        await manager.waitForPendingRegistrations()
        XCTAssertEqual(manager.confirmed?.registered, true)
        XCTAssertEqual(manager.confirmed?.revision, 3)
        XCTAssertEqual(manager.confirmed?.preferences, original)
        XCTAssertEqual(manager.errorMessage, failure)
        XCTAssertEqual(manager.unconfirmedPreferences, desired)
        await manager.reconcile() // A newer registration revision with the old preferences is insufficient.
        XCTAssertEqual(manager.errorMessage, failure)
        XCTAssertEqual(manager.unconfirmedPreferences, desired)
        XCTAssertEqual(manager.confirmed?.preferences, original)
        XCTAssertEqual(relay.attempts, 1, "Registration recovery reuses its pending grant")
        XCTAssertEqual(system.permissionRequests, 0)
        await manager.waitForPendingRegistrations()
    }
}

extension PushPreferenceRecoveryTests {
    func testSuccessOptInAndOptOutRequireConfirmationAndKeepRejectedDraftVisible() async throws {
        let http = PreferenceRecoveryHTTP()
        PreferenceRecoveryURLProtocol.fixture = http
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PreferenceRecoveryURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        system.status = .denied
        let manager = manager(auth, system: system)
        await manager.reconcile()
        XCTAssertEqual(manager.confirmed?.preferences.taskSuccess, false)
        XCTAssertEqual(manager.confirmed?.tasksAllowed, true)
        for success in [true, false] {
            let original = try XCTUnwrap(manager.confirmed?.preferences)
            var desired = original
            desired.taskSuccess = success
            http.rejectSave()
            await manager.savePreferences(desired)
            let failure = try XCTUnwrap(manager.errorMessage)
            await manager.reconcile()
            await manager.retry()
            XCTAssertEqual(manager.confirmed?.preferences, original)
            XCTAssertEqual(manager.unconfirmedPreferences?.taskSuccess, success)
            XCTAssertEqual(manager.errorMessage, failure)
            http.acceptSave()
            await manager.savePreferences(desired)
            XCTAssertEqual(manager.confirmed?.preferences, desired)
            XCTAssertNil(manager.errorMessage)
            XCTAssertNil(manager.unconfirmedPreferences)
        }
        XCTAssertEqual(http.savedPreferences().map(\.taskSuccess), [true, true, false, false])
        XCTAssertEqual(system.permissionRequests, 0)
        await manager.waitForPendingRegistrations()
    }

    func testTaskSubscriptionsUseCurrentServerPermissionWhenSavingAfterDemotion() async throws {
        let http = PreferenceRecoveryHTTP()
        PreferenceRecoveryURLProtocol.fixture = http
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PreferenceRecoveryURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let manager = manager(auth, system: TestPushSystem())
        await manager.reconcile()
        var draft = try XCTUnwrap(manager.confirmed?.preferences)
        XCTAssertEqual(auth.user?.role, "member", "Cached role does not replace current Server permission")
        XCTAssertEqual(manager.confirmed?.tasksAllowed, true)
        draft.taskFailure = true
        draft.taskSuccess = true
        http.revokeTaskAccess()
        await manager.reconcile()
        XCTAssertEqual(manager.confirmed?.tasksAllowed, false)
        await manager.savePreferences(draft)
        XCTAssertEqual(manager.confirmed?.preferences.taskFailure, false)
        XCTAssertEqual(manager.confirmed?.preferences.taskSuccess, false)
        XCTAssertEqual(http.savedPreferences().last?.taskFailure, false)
        XCTAssertEqual(http.savedPreferences().last?.taskSuccess, false)
        await manager.waitForPendingRegistrations()
    }
}
