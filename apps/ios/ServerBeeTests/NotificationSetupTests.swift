import Foundation
import XCTest
@testable import ServerBee

private final class SetupHTTPFixture: @unchecked Sendable {
    private let lock = NSLock()
    private var setup = PushSetupTestData.response(enabled: false, revision: 0)
    private var saveStatus = 200
    private var registerStatus = 200
    private var requests: [URLRequest] = []
    private var registrationCallback: (@Sendable () -> Void)?
    var registered: (@Sendable () -> Void)? {
        get { lock.lock(); defer { lock.unlock() }; return registrationCallback }
        set { lock.lock(); defer { lock.unlock() }; registrationCallback = newValue }
    }

    func failSave() { lock.lock(); defer { lock.unlock() }; saveStatus = 503 }
    func setRegistrationStatus(_ value: Int) { lock.lock(); defer { lock.unlock() }; registerStatus = value }
    func enable() { lock.lock(); defer { lock.unlock() }; setup = PushSetupTestData.response() }
    func snapshot() -> [URLRequest] { lock.lock(); defer { lock.unlock() }; return requests }

    func handle(_ request: URLRequest) -> (Int, Data) {
        lock.lock()
        defer { lock.unlock() }
        requests.append(request)
        switch (request.url?.path, request.httpMethod) {
        case ("/api/mobile/push/settings", "GET"): return (200, setup)
        case ("/api/mobile/push/settings", "PUT"):
            if saveStatus != 200 { return (saveStatus, Data(#"{"error":{"message":"fixture save rejected"}}"#.utf8)) }
            setup = PushSetupTestData.response()
            return (200, setup)
        case ("/api/mobile/push/verified-register", "POST"):
            if registerStatus == 200 { setup = PushSetupTestData.response(registered: true, revision: 2) }
            registrationCallback?()
            return (registerStatus, setup)
        default: return (200, Data(#"{"data":"ok"}"#.utf8))
        }
    }
}

private final class SetupURLProtocol: URLProtocol {
    nonisolated(unsafe) static var fixture: SetupHTTPFixture?
    override static func canInit(with request: URLRequest) -> Bool { true }
    override static func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func stopLoading() {}
    override func startLoading() {
        guard let fixture = Self.fixture, let url = request.url else { return }
        let (status, data) = fixture.handle(request)
        guard let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: nil, headerFields: nil) else { return }
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }
}

@MainActor
final class NotificationSetupTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(SetupURLProtocol.self) }
    override func tearDown() async throws {
        URLProtocol.unregisterClass(SetupURLProtocol.self)
        SetupURLProtocol.fixture = nil
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
        let manager = PushNotificationManager(system: system, relay: relay)
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
        let manager = PushNotificationManager(system: system, relay: TestPushRelay())
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
        let manager = PushNotificationManager(system: system, relay: TestPushRelay())
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
        let manager = PushNotificationManager(system: system, relay: relay)
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
        let manager = PushNotificationManager(system: TestPushSystem(), relay: relay)
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
        XCTAssertEqual(relay.attempts, 2)
    }

    func testPermissionCompletionAfterAccountSwitchCannotRegisterReplacement() async {
        let fixture = SetupHTTPFixture()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager()
        login(auth)
        let system = TestPushSystem()
        let manager = PushNotificationManager(system: system, relay: TestPushRelay())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        let suspended = expectation(description: "permission request suspended")
        var continuation: CheckedContinuation<Void, Never>?
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
        await saving.value
        XCTAssertEqual(system.registrations, 0)
        XCTAssertNil(manager.confirmed)
        XCTAssertEqual(auth.user?.id, "bob")
    }
}
