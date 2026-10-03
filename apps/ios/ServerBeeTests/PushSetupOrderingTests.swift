import Foundation
import XCTest
@testable import ServerBee

/// Delays replies only at the HTTP boundary, after capturing the Server result.
private final class OrderedSetupHTTP: @unchecked Sendable {
    private let lock = NSLock()
    private var setup: Data
    private var revision: Int64
    private var preferences: PushPreferences
    private var requests: [URLRequest] = []
    private struct HeldRoute {
        let method: String
        let path: String
        let entered: XCTestExpectation
    }
    private var nextHold: HeldRoute?
    private var held: (request: OrderedSetupURLProtocol, data: Data)?
    private var loseSave = false
    private var registrationEntered: XCTestExpectation?

    init(enabled: Bool = true, registered: Bool = false, revision: Int64 = 1) {
        self.revision = revision
        let initial = PushPreferences(enabled: enabled, alerts: true, security: false, taskFailure: true, taskSuccess: false)
        preferences = initial
        setup = PushSetupTestData.response(registered: registered, revision: revision, preferences: initial)
    }

    func holdNext(_ method: String, path: String, entered: XCTestExpectation) {
        lock.lock(); defer { lock.unlock() }
        nextHold = HeldRoute(method: method, path: path, entered: entered)
    }
    func loseCommittedSave() { lock.lock(); defer { lock.unlock() }; loseSave = true }
    func observeRegistration(_ entered: XCTestExpectation) {
        lock.lock(); defer { lock.unlock() }
        registrationEntered = entered
    }
    func invalidateGrant() {
        lock.lock(); defer { lock.unlock() }
        setup = PushSetupTestData.response(registered: false, revision: revision, preferences: preferences)
    }
    func snapshot() -> [URLRequest] { lock.lock(); defer { lock.unlock() }; return requests }
    func release() {
        lock.lock()
        let reply = held
        held = nil
        lock.unlock()
        if let reply { reply.request.respond(reply.data) }
    }
    // The test takes and releases this non-Sendable request on MainActor; it is
    // never captured by a transferable closure. The fixture handoff stays locked.
    @MainActor
    func takeHeld() -> (request: OrderedSetupURLProtocol, data: Data)? {
        lock.lock()
        let reply = held
        held = nil
        lock.unlock()
        return reply
    }
    func cancel() {
        lock.lock()
        let request = held?.request
        held = nil
        nextHold = nil
        registrationEntered = nil
        lock.unlock()
        request?.fail(URLError(.cancelled))
    }

    func handle(_ request: OrderedSetupURLProtocol) {
        lock.lock()
        let urlRequest = request.request
        requests.append(urlRequest)
        let path = urlRequest.url?.path ?? ""
        var data = setup
        var lost = false
        var registered: XCTestExpectation?
        if path == "/api/mobile/auth/refresh" {
            data = Data(#"""
            {"data":{"access_token":"restored-access","access_expires_in_secs":900,"refresh_token":"restored-refresh",
            "refresh_expires_in_secs":3600,"token_type":"Bearer","user":{"id":"alice","username":"alice","role":"member"}}}
            """#.utf8)
        } else if urlRequest.httpMethod == "PUT" {
            let body = PushSetupTestData.body(urlRequest)
            guard let submitted = body["preferences"] as? [String: Any],
                  let enabled = submitted["enabled"] as? Bool, let alerts = submitted["alerts"] as? Bool,
                  let security = submitted["security"] as? Bool, let taskFailure = submitted["task_failure"] as? Bool,
                  let taskSuccess = submitted["task_success"] as? Bool,
                  let expected = body["expected_revision"] as? NSNumber, expected.int64Value == revision else {
                lock.unlock()
                request.fail(URLError(.badServerResponse))
                return
            }
            preferences = PushPreferences(enabled: enabled, alerts: alerts, security: security, taskFailure: taskFailure, taskSuccess: taskSuccess)
            revision = expected.int64Value + 1
            setup = PushSetupTestData.response(revision: revision, preferences: preferences)
            data = setup
            lost = loseSave
            loseSave = false
        } else if path == "/api/mobile/push/verified-register" {
            registered = registrationEntered
            registrationEntered = nil
            revision = (PushSetupTestData.body(urlRequest)["expected_revision"] as? NSNumber)?.int64Value ?? revision
            revision += 1
            setup = PushSetupTestData.response(registered: true, revision: revision, preferences: preferences)
            data = setup
        } else if path == "/api/mobile/push/unregister" {
            data = Data(#"{"data":"ok"}"#.utf8)
        }
        if let hold = nextHold, hold.method == urlRequest.httpMethod, hold.path == path {
            held = (request, data)
            nextHold = nil
            lock.unlock()
            registered?.fulfill()
            hold.entered.fulfill()
            return
        }
        lock.unlock()
        registered?.fulfill()
        if lost { request.fail(URLError(.networkConnectionLost)) } else { request.respond(data) }
    }
}

private final class OrderedSetupURLProtocol: URLProtocol {
    nonisolated(unsafe) static var fixture: OrderedSetupHTTP?
    private static let pending = PendingURLProtocolRequests()
    override static func canInit(with request: URLRequest) -> Bool { true }
    override static func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        Self.pending.begin(self)
        if let fixture = Self.fixture { fixture.handle(self) } else { fail(URLError(.cancelled)) }
    }
    override func stopLoading() { _ = Self.pending.finish(self) }
    static func cancelPending() { pending.cancelAll() }
    func respond(_ data: Data) {
        guard let url = request.url,
              let response = HTTPURLResponse(url: url, statusCode: 200, httpVersion: nil, headerFields: nil) else {
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
final class PushSetupOrderingTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(OrderedSetupURLProtocol.self) }
    override func tearDown() async throws {
        OrderedSetupURLProtocol.fixture?.cancel()
        OrderedSetupURLProtocol.fixture = nil
        OrderedSetupURLProtocol.cancelPending()
        URLProtocol.unregisterClass(OrderedSetupURLProtocol.self)
        AuthManager().clearAuth()
        try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
    }
    private func login(_ auth: AuthManager, server: String = "https://ordering.test", user: String = "alice") {
        auth.setServerUrl(server)
        auth.handleLoginResponse(MobileTokenResponse(
            accessToken: "access-\(UUID().uuidString)", accessExpiresInSecs: 900,
            refreshToken: "refresh-\(UUID().uuidString)", refreshExpiresInSecs: 3600,
            tokenType: "Bearer", user: MobileUser(id: user, username: user, role: "member"), revocationToken: "proof-\(UUID().uuidString)",
            mobileSessionId: UUID().uuidString))
    }
    private func manager(_ auth: AuthManager, system: TestPushSystem, relay: TestPushRelay) -> PushNotificationManager {
        let manager = PushNotificationManager(system: system, relay: relay, storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        return manager
    }
    private func registerToken(_ manager: PushNotificationManager, expectUpload: Bool = true) async {
        let entered = expectUpload ? expectation(description: "token uploaded through HTTP") : nil
        if let entered { OrderedSetupURLProtocol.fixture?.observeRegistration(entered) }
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 10, count: 32))
        if let entered { await fulfillment(of: [entered], timeout: 3) } else { await Task.yield() }
        XCTAssertNotNil(manager.deviceToken)
        await manager.waitForPendingRegistrations()
    }

    func testHeldEnabledGetCannotRestoreStateAfterSuccessfulDisable() async {
        let http = OrderedSetupHTTP()
        defer { http.cancel() }
        OrderedSetupURLProtocol.fixture = http
        let auth = AuthManager()
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        let relay = TestPushRelay()
        let manager = manager(auth, system: system, relay: relay)
        await manager.reconcile()
        await registerToken(manager)
        XCTAssertEqual(manager.confirmed?.registered, true)
        let entered = expectation(description: "old enabled GET captured")
        http.holdNext("GET", path: "/api/mobile/push/settings", entered: entered)
        let oldRead = Task { await manager.reconcile() }
        await fulfillment(of: [entered], timeout: 3)
        var preferences = PushPreferences()
        preferences.enabled = false
        await manager.savePreferences(preferences)
        XCTAssertEqual(manager.confirmed?.revision, 3)
        XCTAssertEqual(manager.confirmed?.preferences.enabled, false)
        XCTAssertEqual(relay.revocations, 1)
        let registrations = system.registrations
        http.release()
        await oldRead.value
        await registerToken(manager, expectUpload: false) // Same previously confirmed token.
        XCTAssertEqual(manager.confirmed?.revision, 3)
        XCTAssertEqual(manager.confirmed?.preferences.enabled, false)
        XCTAssertEqual(manager.confirmed?.registered, false)
        XCTAssertEqual(system.registrations, registrations)
        XCTAssertEqual(relay.attempts, 1)
        XCTAssertEqual(http.snapshot().filter { $0.url?.path == "/api/mobile/push/verified-register" }.count, 1)
    }

    func testHeldGetCannotConfirmAnOverlappingUploadOrReplaceItsNewRevision() async {
        for releaseBeforeUpload in [true, false] {
            let http = OrderedSetupHTTP()
            defer { http.cancel() }
            OrderedSetupURLProtocol.fixture = http
            let auth = AuthManager()
            auth.clearAuth()
            login(auth)
            let system = TestPushSystem()
            let relay = TestPushRelay()
            let manager = manager(auth, system: system, relay: relay)
            await manager.reconcile()
            await registerToken(manager)
            let readEntered = expectation(description: "old GET captured before renewal")
            http.holdNext("GET", path: "/api/mobile/push/settings", entered: readEntered)
            let oldRead = Task { await manager.reconcile() }
            await fulfillment(of: [readEntered], timeout: 3)
            // Capture the GET reply separately, then hold the registration reply.
            // Releasing the GET below must not affect the active upload.
            let uploadEntered = expectation(description: "renewal HTTP response held")
            let oldReply = http.takeHeld()
            XCTAssertNotNil(oldReply)
            http.holdNext("POST", path: "/api/mobile/push/verified-register", entered: uploadEntered)
            await manager.retry()
            await fulfillment(of: [uploadEntered], timeout: 3)
            XCTAssertEqual(manager.confirmed?.registered, false)
            if releaseBeforeUpload {
                if let oldReply { oldReply.request.respond(oldReply.data) }
                await oldRead.value
                XCTAssertEqual(manager.confirmed?.registered, false)
                XCTAssertTrue(manager.isSaving)
            }
            http.release()
            await manager.waitForPendingRegistrations()
            if !releaseBeforeUpload {
                if let oldReply { oldReply.request.respond(oldReply.data) }
                await oldRead.value
            }
            XCTAssertEqual(manager.confirmed?.revision, 3)
            XCTAssertEqual(manager.confirmed?.registered, true)
            XCTAssertNil(manager.errorMessage)
            XCTAssertEqual(relay.attempts, 2)
            await manager.unregister()
            auth.clearAuth()
        }
    }

    func testNewestGetWinsWhenGrantInspectionChangesAtTheSameRevision() async {
        let http = OrderedSetupHTTP(registered: true, revision: 2)
        defer { http.cancel() }
        OrderedSetupURLProtocol.fixture = http
        let auth = AuthManager()
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        let manager = manager(auth, system: system, relay: TestPushRelay())
        await manager.reconcile()
        let entered = expectation(description: "old confirmed grant GET held")
        http.holdNext("GET", path: "/api/mobile/push/settings", entered: entered)
        let oldRead = Task { await manager.reconcile() }
        await fulfillment(of: [entered], timeout: 3)
        http.invalidateGrant()
        await manager.reconcile()
        XCTAssertEqual(manager.confirmed?.registered, false)
        http.release()
        await oldRead.value
        XCTAssertEqual(manager.confirmed?.revision, 2)
        XCTAssertEqual(manager.confirmed?.registered, false)
    }

    func testReadAndTokenCallbackDuringHeldPreferenceSaveCannotStartUpload() async {
        let http = OrderedSetupHTTP()
        defer { http.cancel() }
        OrderedSetupURLProtocol.fixture = http
        let auth = AuthManager()
        auth.clearAuth()
        login(auth)
        let relay = TestPushRelay()
        let manager = manager(auth, system: TestPushSystem(), relay: relay)
        await manager.reconcile()
        let entered = expectation(description: "disable PUT committed but reply held")
        http.holdNext("PUT", path: "/api/mobile/push/settings", entered: entered)
        let saving = Task { await manager.savePreferences(PushPreferences()) }
        await fulfillment(of: [entered], timeout: 3)
        let requests = http.snapshot().count
        await manager.reconcile()
        await registerToken(manager, expectUpload: false)
        XCTAssertEqual(http.snapshot().count, requests)
        XCTAssertEqual(relay.attempts, 0)
        XCTAssertTrue(manager.isSaving)
        http.release()
        await saving.value
        await manager.reconcile()
        XCTAssertEqual(manager.confirmed?.revision, 2)
        XCTAssertEqual(manager.confirmed?.preferences.enabled, false)
        XCTAssertEqual(manager.confirmed?.registered, false)
        XCTAssertEqual(relay.attempts, 0)
    }

    func testExplicitRetryRequestsPermissionAfterCommittedEnableResponseLoss() async {
        let http = OrderedSetupHTTP(enabled: false, revision: 0)
        OrderedSetupURLProtocol.fixture = http
        let auth = AuthManager()
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        system.status = .notDetermined
        system.permissionHook = { system.status = .authorized }
        let relay = TestPushRelay()
        let manager = manager(auth, system: system, relay: relay)
        await manager.reconcile()
        var preferences = PushPreferences()
        preferences.enabled = true
        http.loseCommittedSave()
        await manager.savePreferences(preferences)
        XCTAssertEqual(manager.confirmed?.preferences.enabled, false)
        XCTAssertNotNil(manager.errorMessage)
        XCTAssertEqual(system.permissionRequests, 0)
        await manager.reconcile()
        XCTAssertEqual(manager.confirmed?.preferences.enabled, true)
        XCTAssertEqual(system.permissionRequests, 0)
        await manager.retry()
        XCTAssertEqual(system.permissionRequests, 1)
        XCTAssertEqual(system.registrations, 1)
        await registerToken(manager)
        XCTAssertEqual(manager.confirmed?.registered, true)
        XCTAssertNil(manager.errorMessage)
        XCTAssertEqual(relay.attempts, 1)
        await manager.unregister()
    }

    func testRestoredEnabledManagerNeverPromptsUntilExplicitRetry() async {
        let http = OrderedSetupHTTP()
        OrderedSetupURLProtocol.fixture = http
        let previous = AuthManager()
        login(previous)
        let restored = AuthManager()
        await restored.initialize()
        XCTAssertTrue(restored.isAuthenticated)
        XCTAssertEqual(restored.captureContext()?.pushScope, previous.captureContext()?.pushScope)
        let system = TestPushSystem()
        system.status = .notDetermined
        system.permissionHook = { system.status = .authorized }
        let manager = manager(restored, system: system, relay: TestPushRelay())
        await manager.reconcile()
        await manager.reconcile()
        XCTAssertEqual(manager.confirmed?.preferences.enabled, true)
        XCTAssertEqual(system.permissionRequests, 0)
        XCTAssertEqual(system.registrations, 0)
        await manager.retry()
        XCTAssertEqual(system.permissionRequests, 1)
        XCTAssertEqual(system.registrations, 1)
        XCTAssertTrue(manager.permissionGranted)
        await manager.retry()
        XCTAssertEqual(system.permissionRequests, 1)
    }

    func testRetryDoesNotPromptForDeniedPermission() async {
        let http = OrderedSetupHTTP()
        OrderedSetupURLProtocol.fixture = http
        let auth = AuthManager()
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        system.status = .denied
        let relay = TestPushRelay()
        let manager = manager(auth, system: system, relay: relay)
        await manager.reconcile()
        await manager.retry()
        XCTAssertEqual(system.permissionRequests, 0)
        XCTAssertEqual(system.registrations, 0)
        XCTAssertEqual(relay.attempts, 0)
        XCTAssertFalse(manager.permissionGranted)
    }

}

extension PushSetupOrderingTests {
    func testOldRetryCannotContinuePermissionAfterLoginChangesDuringUploadOrRead() async {
        for phase in ["upload", "read"] {
            for (server, user) in [
                ("https://ordering.test", "bob"), ("https://replacement.test", "alice"), ("https://ordering.test", "alice")
            ] {
                await replacementDuringRetry(phase: phase, server: server, user: user)
            }
        }
    }

    private func replacementDuringRetry(phase: String, server: String, user: String) async {
        let original = OrderedSetupHTTP()
        let replacement = OrderedSetupHTTP()
        defer { original.cancel(); replacement.cancel() }
        OrderedSetupURLProtocol.fixture = original
        let auth = AuthManager()
        auth.clearAuth()
        login(auth)
        let originalGeneration = auth.authenticationGeneration
        let originalScope = auth.captureContext()?.pushScope
        XCTAssertNotNil(originalScope)
        let system = TestPushSystem()
        let relay = TestPushRelay()
        let manager = manager(auth, system: system, relay: relay)
        if phase == "read" { system.status = .notDetermined }
        await manager.reconcile()
        let entered = expectation(description: "original \(phase) held before replacing login")
        if phase == "upload" {
            original.holdNext("POST", path: "/api/mobile/push/verified-register", entered: entered)
            manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 10, count: 32))
            await fulfillment(of: [entered], timeout: 3)
        } else {
            original.holdNext("GET", path: "/api/mobile/push/settings", entered: entered)
        }
        let retryStarted = expectation(description: "old explicit Retry started")
        let retry = Task { @MainActor in
            retryStarted.fulfill()
            await manager.retry()
        }
        await fulfillment(of: phase == "read" ? [retryStarted, entered] : [retryStarted], timeout: 3)
        auth.clearAuth()
        login(auth, server: server, user: user)
        XCTAssertNotEqual(auth.authenticationGeneration, originalGeneration)
        let replacementScope = auth.captureContext()?.pushScope
        XCTAssertNotNil(replacementScope)
        XCTAssertNotEqual(replacementScope, originalScope)
        manager.configure(apiClient: APIClient(authManager: auth))
        OrderedSetupURLProtocol.fixture = replacement
        system.status = .notDetermined
        await manager.reconcile()
        XCTAssertEqual(manager.confirmed?.revision, 1)
        let requests = replacement.snapshot().count
        original.release()
        await retry.value
        await manager.waitForPendingRegistrations()
        XCTAssertEqual(replacement.snapshot().count, requests, "Old Retry must not even reconcile the replacement login")
        XCTAssertTrue(replacement.snapshot().allSatisfy { $0.httpMethod == "GET" && $0.url?.host == URL(string: server)?.host })
        XCTAssertEqual(system.permissionRequests, 0)
        XCTAssertEqual(relay.attempts, phase == "upload" ? 1 : 0)
        XCTAssertEqual(manager.confirmed?.revision, 1)
        XCTAssertEqual(manager.confirmed?.preferences.enabled, true)
        XCTAssertEqual(manager.confirmed?.registered, false)
        XCTAssertNil(manager.errorMessage)
        XCTAssertEqual(auth.user?.id, user)
        XCTAssertEqual(auth.serverUrl, server)
        XCTAssertTrue(auth.isAuthenticated)
        auth.clearAuth()
    }
}
