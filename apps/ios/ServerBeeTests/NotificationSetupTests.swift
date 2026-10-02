import Foundation
import XCTest
@testable import ServerBee

private final class SetupHTTPFixture: @unchecked Sendable {
    private let lock = NSLock()
    private var setup = PushSetupTestData.response(enabled: false, revision: 0)
    private var revision: Int64 = 0
    private var saveStatus = 200
    private var registerStatus = 200
    private var requests: [URLRequest] = []
    private var grantIDs: [String] = []
    private var registrationCallback: (@Sendable () -> Void)?
    var registered: (@Sendable () -> Void)? {
        get { lock.lock(); defer { lock.unlock() }; return registrationCallback }
        set { lock.lock(); defer { lock.unlock() }; registrationCallback = newValue }
    }

    func failSave() { lock.lock(); defer { lock.unlock() }; saveStatus = 503 }
    func setRegistrationStatus(_ value: Int) { lock.lock(); defer { lock.unlock() }; registerStatus = value }
    func confirm() { lock.lock(); defer { lock.unlock() }; setup = PushSetupTestData.response(registered: true, revision: revision) }
    func enable() {
        lock.lock(); defer { lock.unlock() }
        revision = 1
        setup = PushSetupTestData.response(revision: revision)
    }
    func invalidateGrant() {
        lock.lock(); defer { lock.unlock() }
        setup = PushSetupTestData.response(registered: false, revision: revision)
    }
    func registeredGrants() -> [String] { lock.lock(); defer { lock.unlock() }; return grantIDs }
    func snapshot() -> [URLRequest] { lock.lock(); defer { lock.unlock() }; return requests }

    func handle(_ request: URLRequest) -> (Int, Data) {
        lock.lock()
        defer { lock.unlock() }
        requests.append(request)
        switch (request.url?.path, request.httpMethod) {
        case ("/api/mobile/auth/refresh", "POST"):
            return (200, Data("""
            {"data":{"access_token":"restored-access","access_expires_in_secs":900,"refresh_token":"restored-refresh",
            "refresh_expires_in_secs":3600,"token_type":"Bearer","user":{"id":"alice","username":"alice","role":"member"}}}
            """.utf8))
        case ("/api/mobile/push/settings", "GET"): return (200, setup)
        case ("/api/mobile/push/settings", "PUT"):
            if saveStatus != 200 { return (saveStatus, Data(#"{"error":{"message":"fixture save rejected"}}"#.utf8)) }
            let body = PushSetupTestData.body(request)
            guard let expected = body["expected_revision"] as? NSNumber, expected.int64Value == revision,
                  let preferences = body["preferences"] as? [String: Any], let enabled = preferences["enabled"] as? Bool else {
                return (409, Data(#"{"error":{"message":"fixture save revision or preferences invalid"}}"#.utf8))
            }
            revision = expected.int64Value + 1
            setup = PushSetupTestData.response(enabled: enabled, revision: revision)
            return (200, setup)
        case ("/api/mobile/push/verified-register", "POST"):
            if let grant = PushSetupTestData.body(request)["grant_id"] as? String { grantIDs.append(grant) }
            if registerStatus == 200 || registerStatus == -2 {
                guard let expected = PushSetupTestData.body(request)["expected_revision"] as? NSNumber,
                      expected.int64Value == revision else {
                    registrationCallback?()
                    return (409, Data(#"{"error":{"message":"fixture registration revision invalid"}}"#.utf8))
                }
                revision = expected.int64Value + 1
                setup = PushSetupTestData.response(registered: true, revision: revision)
            }
            registrationCallback?()
            return (registerStatus == -2 ? -1 : registerStatus, setup)
        default: return (200, Data(#"{"data":"ok"}"#.utf8))
        }
    }
}

private final class SetupURLProtocol: URLProtocol {
    nonisolated(unsafe) static var fixture: SetupHTTPFixture?
    override static func canInit(with request: URLRequest) -> Bool { true }
    override static func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    private static let pending = PendingURLProtocolRequests()
    override func stopLoading() { _ = Self.pending.finish(self) }
    static func cancelPending() { pending.cancelAll() }
    override func startLoading() {
        Self.pending.begin(self)
        guard let fixture = Self.fixture, let url = request.url else {
            fail(URLError(.cancelled))
            return
        }
        let (status, data) = fixture.handle(request)
        if status == -1 {
            fail(URLError(.networkConnectionLost))
            return
        }
        guard let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: nil, headerFields: nil) else {
            fail(URLError(.badServerResponse))
            return
        }
        guard Self.pending.finish(self) else { return }
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }
    private func fail(_ error: Error) {
        guard Self.pending.finish(self) else { return }
        client?.urlProtocol(self, didFailWithError: error)
    }
}

@MainActor
final class NotificationSetupTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(SetupURLProtocol.self) }
    override func tearDown() async throws {
        SetupURLProtocol.fixture = nil
        SetupURLProtocol.cancelPending()
        URLProtocol.unregisterClass(SetupURLProtocol.self)
        AuthManager().clearAuth()
    }

    private func login(_ auth: AuthManager, user: String = "alice") {
        auth.setServerUrl("https://\(user).test")
        auth.handleLoginResponse(MobileTokenResponse(
            accessToken: "access-\(user)", accessExpiresInSecs: 900, refreshToken: "refresh-\(user)", refreshExpiresInSecs: 3600,
            tokenType: "Bearer", user: MobileUser(id: user, username: user, role: "member")
        ))
    }

    func testLaunchAndRepeatedReconciliationNeverPromptWithoutExplicitOptIn() async {
        let fixture = SetupHTTPFixture()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager()
        login(auth)
        let system = TestPushSystem()
        let relay = TestPushRelay()
        let manager = PushNotificationManager(system: system, relay: relay, storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        await manager.reconcile()
        XCTAssertEqual(system.permissionRequests, 0)
        XCTAssertEqual(system.registrations, 0)
        XCTAssertEqual(relay.attempts, 0)
        XCTAssertEqual(manager.confirmed?.preferences.enabled, false)
        XCTAssertTrue(fixture.snapshot().allSatisfy { $0.httpMethod == "GET" })
    }

    func testExplicitEnableSavesIntentBeforePromptAndServerConfirmsRegistration() async {
        let fixture = SetupHTTPFixture()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager()
        login(auth)
        let system = TestPushSystem()
        let manager = PushNotificationManager(system: system, relay: TestPushRelay(), storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        var preferences = PushPreferences()
        preferences.enabled = true
        preferences.alerts = true
        system.permissionHook = { XCTAssertEqual(fixture.snapshot().last?.httpMethod, "PUT") }
        await manager.savePreferences(preferences)
        XCTAssertEqual(system.permissionRequests, 1)
        let uploaded = expectation(description: "Server registration")
        fixture.registered = { uploaded.fulfill() }
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 10, count: 32))
        await fulfillment(of: [uploaded], timeout: 3)
        await manager.waitForPendingRegistrations()
        XCTAssertEqual(manager.confirmed?.registered, true)
        XCTAssertEqual(manager.confirmed?.deliveryAvailable, false)
        await manager.unregister()
        let register = fixture.snapshot().first { $0.url?.path == "/api/mobile/push/verified-register" }
        XCTAssertEqual(register?.value(forHTTPHeaderField: "Authorization"), "Bearer access-alice")
        XCTAssertTrue(fixture.snapshot().allSatisfy { $0.url?.host == "alice.test" })
    }

    func testFailedSaveRetainsConfirmedPreferencesAndDoesNotPrompt() async {
        let fixture = SetupHTTPFixture()
        fixture.failSave()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager()
        login(auth)
        let system = TestPushSystem()
        let manager = PushNotificationManager(system: system, relay: TestPushRelay(), storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        var preferences = PushPreferences()
        preferences.enabled = true
        await manager.savePreferences(preferences)
        XCTAssertEqual(manager.confirmed?.preferences.enabled, false)
        XCTAssertEqual(manager.confirmed?.revision, 0)
        XCTAssertNotNil(manager.errorMessage)
        XCTAssertEqual(system.permissionRequests, 0)
    }

    func testUnsupportedAttestationPreservesLoginAndReportsUnavailable() async {
        let fixture = SetupHTTPFixture()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager()
        login(auth)
        let system = TestPushSystem()
        let relay = TestPushRelay()
        relay.supported = false
        let manager = PushNotificationManager(system: system, relay: relay, storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        var preferences = PushPreferences()
        preferences.enabled = true
        await manager.savePreferences(preferences)
        XCTAssertTrue(manager.verificationUnavailable)
        XCTAssertEqual(manager.confirmed?.registered, false)
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertEqual(system.permissionRequests, 0)
        XCTAssertEqual(relay.attempts, 0)
    }

    func testEarlyCallbackWaitsForConfirmedIntentAndRecoveryRetriesFailedUpload() async {
        let fixture = SetupHTTPFixture()
        fixture.enable()
        fixture.setRegistrationStatus(503)
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager()
        login(auth)
        let relay = TestPushRelay()
        let manager = PushNotificationManager(system: TestPushSystem(), relay: relay, storage: MemoryPushSetupStorage())
        // Delegate token arrived before stores/authentication were installed.
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 10, count: 32))
        await Task.yield()
        XCTAssertEqual(relay.attempts, 0)
        manager.configure(apiClient: APIClient(authManager: auth))
        let failed = expectation(description: "first registration rejected")
        fixture.registered = { failed.fulfill() }
        await manager.reconcile()
        await fulfillment(of: [failed], timeout: 3)
        // Reconcile after connectivity recovery reads state and retries.
        let recovered = expectation(description: "registration recovered")
        fixture.registered = { recovered.fulfill() }
        fixture.setRegistrationStatus(200)
        await manager.waitForPendingRegistrations()
        XCTAssertNotNil(manager.errorMessage)
        await manager.reconcile()
        await fulfillment(of: [recovered], timeout: 3)
        await manager.unregister()
        XCTAssertEqual(relay.attempts, 1, "Retry saves the already verified pending grant without another rotation")
    }

    func testPermissionCompletionAfterAccountSwitchCannotRegisterReplacement() async {
        let fixture = SetupHTTPFixture()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager()
        login(auth)
        let system = TestPushSystem()
        let manager = PushNotificationManager(system: system, relay: TestPushRelay(), storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        let suspended = expectation(description: "permission request suspended")
        var continuation: CheckedContinuation<Void, Never>?
        defer { continuation?.resume() }
        system.permissionHook = {
            await withCheckedContinuation { continuation = $0; suspended.fulfill() }
        }
        var preferences = PushPreferences()
        preferences.enabled = true
        let saving = Task { await manager.savePreferences(preferences) }
        await fulfillment(of: [suspended], timeout: 3)
        auth.clearAuth()
        login(auth, user: "bob")
        manager.configure(apiClient: APIClient(authManager: auth))
        continuation?.resume()
        continuation = nil
        await saving.value
        XCTAssertEqual(system.registrations, 0)
        XCTAssertNil(manager.confirmed)
        XCTAssertEqual(auth.user?.id, "bob")
    }
    func testRotatedGrantSaveFailureStaysUnconfirmedThroughForegroundAndRestart() async throws {
        for failure in [503, -1, -2] {
            let fixture = SetupHTTPFixture()
            fixture.enable()
            SetupURLProtocol.fixture = fixture
            let auth = AuthManager()
            login(auth)
            let storage = MemoryPushSetupStorage()
            let relay = TestPushRelay()
            let manager = PushNotificationManager(system: TestPushSystem(), relay: relay, storage: storage)
            manager.configure(apiClient: APIClient(authManager: auth))
            await manager.reconcile()
            let admitted = expectation(description: "initial Server confirmation")
            fixture.registered = { admitted.fulfill() }
            manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 10, count: 32))
            await fulfillment(of: [admitted], timeout: 3)
            await manager.waitForPendingRegistrations()
            XCTAssertEqual(manager.confirmed?.registered, true)
            XCTAssertEqual(relay.attempts, 1)
            fixture.setRegistrationStatus(failure)
            let failed = expectation(description: "rotated grant save failed")
            fixture.registered = { failed.fulfill() }
            await manager.retry()
            await fulfillment(of: [failed], timeout: 3)
            await manager.waitForPendingRegistrations()
            XCTAssertEqual(relay.attempts, 2)
            XCTAssertEqual(manager.confirmed?.registered, false)
            XCTAssertNotNil(manager.errorMessage)
            // Even a stale Server response claiming a long-lived old grant must
            // not erase the pending rotation or produce Setup confirmed.
            fixture.confirm()
            let foreground = expectation(description: "foreground resubmits pending grant")
            fixture.registered = { foreground.fulfill() }
            await manager.reconcile()
            await fulfillment(of: [foreground], timeout: 3)
            await manager.waitForPendingRegistrations()
            XCTAssertEqual(manager.confirmed?.registered, false)
            XCTAssertNotNil(manager.errorMessage)
            XCTAssertEqual(relay.attempts, 2, "Pending grant is reused instead of rotating again")
            // Restore both authentication and setup coordinator from persistence.
            let restoredAuth = AuthManager()
            await restoredAuth.initialize()
            XCTAssertTrue(restoredAuth.isAuthenticated)
            XCTAssertEqual(restoredAuth.captureContext()?.pushScope, auth.captureContext()?.pushScope)
            let restarted = PushNotificationManager(system: TestPushSystem(), relay: relay, storage: storage)
            restarted.configure(apiClient: APIClient(authManager: restoredAuth))
            await restarted.reconcile()
            XCTAssertEqual(restarted.confirmed?.registered, false)
            XCTAssertNotNil(restarted.errorMessage)
            let recovered = expectation(description: "restart confirms the pending grant")
            fixture.registered = { recovered.fulfill() }
            fixture.setRegistrationStatus(200)
            restarted.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 10, count: 32))
            await fulfillment(of: [recovered], timeout: 3)
            await restarted.waitForPendingRegistrations()
            XCTAssertEqual(restarted.confirmed?.registered, true)
            XCTAssertNil(restarted.errorMessage)
            XCTAssertEqual(relay.attempts, 2)
            XCTAssertTrue(storage.values.isEmpty)
            let saved = fixture.snapshot().filter { $0.url?.path == "/api/mobile/push/verified-register" }
            XCTAssertEqual(saved.count, 4)
            XCTAssertEqual(fixture.registeredGrants(), ["fixture-grant-1", "fixture-grant-2", "fixture-grant-2", "fixture-grant-2"])
            // End all work before changing the global fixture for the next case.
            await restarted.unregister()
            auth.clearAuth()
        }
    }

}

extension NotificationSetupTests {
    func testLostRelayRenewalResponseCannotLeaveSetupConfirmed() async {
        let fixture = SetupHTTPFixture()
        fixture.enable()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager()
        login(auth)
        let relay = TestPushRelay()
        let manager = PushNotificationManager(system: TestPushSystem(), relay: relay, storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        let admitted = expectation(description: "initial registration confirmed")
        fixture.registered = { admitted.fulfill() }
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 10, count: 32))
        await fulfillment(of: [admitted], timeout: 3)
        await manager.waitForPendingRegistrations()
        XCTAssertEqual(manager.confirmed?.registered, true)
        XCTAssertEqual(manager.confirmed?.revision, 2)
        relay.registerHook = { throw URLError(.networkConnectionLost) }
        await manager.retry()
        await manager.waitForPendingRegistrations()
        XCTAssertEqual(manager.confirmed?.registered, false)
        XCTAssertEqual(manager.confirmed?.revision, 2)
        XCTAssertNotNil(manager.errorMessage)
        XCTAssertEqual(relay.attempts, 2)
        XCTAssertEqual(fixture.registeredGrants(), ["fixture-grant-1"])
        relay.registerHook = nil
        fixture.invalidateGrant() // Server inspection invalidates admission without changing revision.
        let recovered = expectation(description: "unknown Relay outcome recovered")
        fixture.registered = { recovered.fulfill() }
        await manager.reconcile()
        await fulfillment(of: [recovered], timeout: 3)
        await manager.waitForPendingRegistrations()
        XCTAssertEqual(manager.confirmed?.registered, true)
        XCTAssertEqual(manager.confirmed?.revision, 3)
        XCTAssertNil(manager.errorMessage)
        let registrations = fixture.snapshot().filter { $0.url?.path == "/api/mobile/push/verified-register" }
        XCTAssertEqual(registrations.count, 2)
        let expected = registrations.last.flatMap { PushSetupTestData.body($0)["expected_revision"] as? NSNumber }
        XCTAssertEqual(expected?.int64Value, 2)
        await manager.unregister()
    }

}
