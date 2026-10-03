import Foundation
import UserNotifications
import XCTest
@testable import ServerBee

private final class SetupHTTPFixture: @unchecked Sendable {
    private let lock = NSLock()
    private var setup = PushSetupTestData.response(enabled: false, revision: 0)
    private var revision: Int64 = 0
    private var selectedPreferences: PushPreferences?
    private var saveStatus = 200
    private var registerStatus = 200
    private var requests: [URLRequest] = []
    private var registrationCallback: (@Sendable () -> Void)?
    var registered: (@Sendable () -> Void)? {
        get { lock.lock(); defer { lock.unlock() }; return registrationCallback }
        set { lock.lock(); defer { lock.unlock() }; registrationCallback = newValue }
    }

    func failSave() { lock.lock(); defer { lock.unlock() }; saveStatus = 503 }
    func loseCommittedSaveReply() { lock.lock(); defer { lock.unlock() }; saveStatus = -2 }
    func setRegistrationStatus(_ value: Int) { lock.lock(); defer { lock.unlock() }; registerStatus = value }
    func enable() {
        lock.lock(); defer { lock.unlock() }
        revision = 1
        setup = PushSetupTestData.response(revision: revision)
    }
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
            if saveStatus != 200 && saveStatus != -2 { return (saveStatus, Data(#"{"error":{"message":"fixture save rejected"}}"#.utf8)) }
            let body = PushSetupTestData.body(request)
            guard let expected = body["expected_revision"] as? NSNumber, expected.int64Value == revision,
                  let preferences = body["preferences"] as? [String: Any], let enabled = preferences["enabled"] as? Bool else {
                return (409, Data(#"{"error":{"message":"fixture save revision or preferences invalid"}}"#.utf8))
            }
            guard let encoded = try? JSONSerialization.data(withJSONObject: preferences),
                  let selected = try? JSONDecoder().decode(PushPreferences.self, from: encoded) else {
                return (400, Data())
            }
            revision = expected.int64Value + 1
            selectedPreferences = selected
            setup = PushSetupTestData.response(enabled: enabled, revision: revision, preferences: selected)
            return (saveStatus == -2 ? -1 : 200, setup)
        case ("/api/mobile/push/encrypted-register", "POST"):
            if registerStatus == 200 || registerStatus == -2 {
                guard let expected = PushSetupTestData.body(request)["expected_revision"] as? NSNumber,
                      expected.int64Value == revision else {
                    registrationCallback?()
                    return (409, Data(#"{"error":{"message":"fixture registration revision invalid"}}"#.utf8))
                }
                revision = expected.int64Value + 1
                setup = PushSetupTestData.response(registered: true, revision: revision, preferences: selectedPreferences)
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
        try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
    }

    private func login(_ auth: AuthManager, user: String = "alice") {
        auth.setServerUrl("https://\(user).test")
        auth.handleLoginResponse(MobileTokenResponse(
            accessToken: "access-\(user)", accessExpiresInSecs: 900, refreshToken: "refresh-\(user)", refreshExpiresInSecs: 3600,
            tokenType: "Bearer", user: MobileUser(id: user, username: user, role: "member"),
            revocationToken: "fixture-deletion-" + UUID().uuidString, mobileSessionId: UUID().uuidString))
    }

    func testLaunchAndRepeatedReconciliationNeverPromptWithoutExplicitOptIn() async {
        let fixture = SetupHTTPFixture()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [SetupURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        let manager = PushNotificationManager(system: system, storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        await manager.reconcile()
        XCTAssertEqual(system.permissionRequests, 0)
        XCTAssertEqual(system.registrations, 0)
        XCTAssertEqual(manager.confirmed?.preferences.enabled, false)
        XCTAssertTrue(registrations(fixture).isEmpty)
        XCTAssertTrue(fixture.snapshot().allSatisfy { $0.httpMethod == "GET" })
    }

    func testExplicitEnableSavesIntentBeforePromptAndServerConfirmsRegistration() async {
        let fixture = SetupHTTPFixture()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [SetupURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        let manager = PushNotificationManager(system: system, storage: MemoryPushSetupStorage())
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
        let register = fixture.snapshot().first { $0.url?.path == "/api/mobile/push/encrypted-register" }
        XCTAssertEqual(register?.value(forHTTPHeaderField: "Authorization"), "Bearer access-alice")
        XCTAssertTrue(fixture.snapshot().allSatisfy { $0.url?.host == "alice.test" })
    }

    func testFailedSaveRetainsConfirmedPreferencesAndDoesNotPrompt() async {
        let fixture = SetupHTTPFixture()
        fixture.failSave()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [SetupURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        let manager = PushNotificationManager(system: system, storage: MemoryPushSetupStorage())
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

    func testEarlyCallbackWaitsForConfirmedIntentAndRecoveryRetriesFailedUpload() async {
        let fixture = SetupHTTPFixture()
        fixture.enable()
        fixture.setRegistrationStatus(503)
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [SetupURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let manager = PushNotificationManager(system: TestPushSystem(), storage: MemoryPushSetupStorage())
        // Delegate token arrived before stores/authentication were installed.
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 10, count: 32))
        await Task.yield()
        XCTAssertTrue(registrations(fixture).isEmpty)
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
        XCTAssertEqual(registrations(fixture).count, 2)
    }

    func testPermissionCompletionAfterAccountSwitchCannotRegisterReplacement() async {
        let fixture = SetupHTTPFixture()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [SetupURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        let manager = PushNotificationManager(system: system, storage: MemoryPushSetupStorage())
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
}

extension NotificationSetupTests {
    func testPermissionRecoveryAndRefreshKeepAllConfirmedCategoriesUntilAccountReplacement() async throws {
        let fixture = SetupHTTPFixture()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [SetupURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let system = TestPushSystem()
        system.status = .denied
        let storage = MemoryPushSetupStorage()
        let manager = PushNotificationManager(system: system, storage: storage)
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        let selected = PushPreferences(enabled: true, alerts: true, security: true, taskFailure: true, taskSuccess: true)
        await manager.savePreferences(selected)
        XCTAssertEqual(manager.confirmed?.preferences, selected)
        XCTAssertFalse(manager.permissionGranted)
        XCTAssertEqual(system.permissionRequests, 1)
        // System settings changed outside the app. Foreground reconciliation
        // recovers registration without requesting permission a second time.
        system.status = .authorized
        await manager.reconcile()
        let uploaded = expectation(description: "all-category installation registered")
        fixture.registered = { uploaded.fulfill() }
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 10, count: 32))
        await fulfillment(of: [uploaded], timeout: 3)
        await manager.waitForPendingRegistrations()
        XCTAssertEqual(manager.confirmed?.registered, true)
        XCTAssertEqual(manager.confirmed?.preferences, selected)
        let key = try XCTUnwrap(manager.contentKey())
        let context = try XCTUnwrap(auth.captureContext())
        let refreshed = try await auth.refreshAccessToken(context: context)
        XCTAssertEqual(refreshed, "restored-access")
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        XCTAssertEqual(manager.confirmed?.preferences, selected)
        XCTAssertEqual(manager.contentKey()?.keyId, key.keyId)
        XCTAssertEqual(manager.contentKey()?.key, key.key)
        XCTAssertEqual(auth.captureContext()?.pushScope, context.pushScope)
        XCTAssertEqual(system.permissionRequests, 1)
        auth.clearAuth()
        login(auth, user: "bob")
        manager.configure(apiClient: APIClient(authManager: auth))
        XCTAssertNil(manager.confirmed)
        XCTAssertNil(manager.contentKey())
        XCTAssertNil(storage.load(PushContentKey.storageKey))
        XCTAssertEqual(auth.user?.id, "bob")
        XCTAssertFalse(fixture.snapshot().contains { $0.url?.host == "bob.test" })
        fixture.registered = nil
    }
}

extension NotificationSetupTests {
    func testRegistrationOnlySendsTokenAndInstallationContentKeyToAuthenticatedServer() async throws {
        let fixture = SetupHTTPFixture()
        fixture.enable()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [SetupURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let manager = PushNotificationManager(system: TestPushSystem(), storage: MemoryPushSetupStorage(), environment: "sandbox")
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        await register(manager, fixture: fixture)
        let requests = registrations(fixture)
        let request = try XCTUnwrap(requests.first)
        let body = PushSetupTestData.body(request)
        XCTAssertEqual(Set(body.keys), Set(["expected_revision", "device_token", "environment", "content_key_id", "content_key", "deployment_id"]))
        XCTAssertEqual(body["device_token"] as? String, String(repeating: "0a", count: 32))
        XCTAssertEqual(body["environment"] as? String, "sandbox")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer access-alice")
        XCTAssertTrue(fixture.snapshot().allSatisfy { $0.url?.host == "alice.test" })
        let key = try XCTUnwrap(manager.contentKey())
        XCTAssertEqual(Data(base64Encoded: key.key)?.count, 32)
        XCTAssertNotNil(UUID(uuidString: key.keyId))
        XCTAssertEqual(body["content_key_id"] as? String, key.keyId)
        XCTAssertEqual(body["content_key"] as? String, key.key)
        XCTAssertEqual(body["deployment_id"] as? String, "https://alice.test")
        XCTAssertEqual(key.scope, auth.captureContext()?.pushScope)
        XCTAssertEqual(key.userId, "alice")
        XCTAssertEqual(key.installationId, auth.captureContext()?.installationId)
        XCTAssertTrue(manager.confirmed?.registered == true)
    }

    func testRejectedAndLostRegistrationRepliesReuseContentKeyAfterRestart() async throws {
        for failure in [503, -1, -2] {
            let fixture = SetupHTTPFixture()
            fixture.enable()
            fixture.setRegistrationStatus(failure)
            SetupURLProtocol.fixture = fixture
            let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [SetupURLProtocol.self]))
            auth.clearAuth()
            login(auth)
            let storage = MemoryPushSetupStorage()
            let manager = PushNotificationManager(system: TestPushSystem(), storage: storage)
            manager.configure(apiClient: APIClient(authManager: auth))
            await manager.reconcile()
            await register(manager, fixture: fixture)
            XCTAssertFalse(manager.confirmed?.registered == true)
            XCTAssertNotNil(manager.errorMessage)
            let key = try XCTUnwrap(manager.contentKey())
            XCTAssertEqual(Set(storage.values.keys), Set([PushContentKey.storageKey]))
            fixture.setRegistrationStatus(200)
            let restarted = PushNotificationManager(system: TestPushSystem(), storage: storage)
            restarted.configure(apiClient: APIClient(authManager: auth))
            await restarted.reconcile()
            await register(restarted, fixture: fixture)
            XCTAssertTrue(restarted.confirmed?.registered == true)
            XCTAssertNil(restarted.errorMessage)
            XCTAssertEqual(restarted.contentKey()?.key, key.key)
            XCTAssertEqual(restarted.contentKey()?.keyId, key.keyId)
            let requests = registrations(fixture)
            XCTAssertEqual(requests.count, 2)
            let expected = requests.map { (PushSetupTestData.body($0)["expected_revision"] as? NSNumber)?.int64Value }
            XCTAssertEqual(expected, failure == -2 ? [1, 2] : [1, 1])
            for request in requests {
                XCTAssertEqual(PushSetupTestData.body(request)["content_key"] as? String, key.key)
                XCTAssertEqual(PushSetupTestData.body(request)["content_key_id"] as? String, key.keyId)
            }
            await restarted.unregister()
            XCTAssertTrue(storage.values.isEmpty)
            auth.clearAuth()
        }
    }

    func testTokenRotationReusesContentKeyAndDuplicateCallbackDoesNotRegister() async throws {
        let fixture = SetupHTTPFixture()
        fixture.enable()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [SetupURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let manager = PushNotificationManager(system: TestPushSystem(), storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        await register(manager, fixture: fixture)
        let key = try XCTUnwrap(manager.contentKey())
        fixture.registered = nil
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 10, count: 32))
        await Task.yield()
        await manager.reconcile()
        await manager.waitForPendingRegistrations()
        XCTAssertEqual(registrations(fixture).count, 1)
        await register(manager, fixture: fixture, byte: 11)
        XCTAssertEqual(registrations(fixture).count, 2)
        XCTAssertEqual(manager.contentKey()?.key, key.key)
        XCTAssertEqual(manager.contentKey()?.keyId, key.keyId)
        XCTAssertEqual(PushSetupTestData.body(try XCTUnwrap(registrations(fixture).last))["device_token"] as? String,
                       String(repeating: "0b", count: 32))
    }

    func testDisableRemovesKeyAndReenableRegistersFreshKey() async throws {
        let fixture = SetupHTTPFixture()
        fixture.enable()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [SetupURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let manager = PushNotificationManager(system: TestPushSystem(), storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        await register(manager, fixture: fixture)
        let key = try XCTUnwrap(manager.contentKey())
        await manager.savePreferences(PushPreferences())
        XCTAssertNil(manager.contentKey())
        XCTAssertEqual(manager.confirmed?.preferences.enabled, false)
        let registered = expectation(description: "re-enabled installation registered")
        fixture.registered = { registered.fulfill() }
        await manager.savePreferences(PushPreferences(enabled: true, alerts: true))
        await fulfillment(of: [registered], timeout: 3)
        await manager.waitForPendingRegistrations()
        XCTAssertTrue(manager.confirmed?.registered == true)
        XCTAssertNotEqual(manager.contentKey()?.keyId, key.keyId)
        XCTAssertNotEqual(manager.contentKey()?.key, key.key)
        XCTAssertEqual(registrations(fixture).count, 2)
    }

    func testLostDisableReplyClearsKeyOnlyWhenServerConfirmsDisabled() async throws {
        let fixture = SetupHTTPFixture()
        fixture.enable()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [SetupURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let manager = PushNotificationManager(system: TestPushSystem(), storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        await register(manager, fixture: fixture)
        let key = try XCTUnwrap(manager.contentKey())
        fixture.loseCommittedSaveReply()
        await manager.savePreferences(PushPreferences())
        XCTAssertNotNil(manager.errorMessage)
        XCTAssertEqual(manager.contentKey()?.keyId, key.keyId)
        await manager.reconcile()
        XCTAssertEqual(manager.confirmed?.preferences.enabled, false)
        XCTAssertNil(manager.errorMessage)
        XCTAssertNil(manager.contentKey())
        XCTAssertEqual(registrations(fixture).count, 1)
    }

    func testOldCleanupCannotRemoveReplacementLoginContentKey() async throws {
        let fixture = SetupHTTPFixture()
        fixture.enable()
        SetupURLProtocol.fixture = fixture
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [SetupURLProtocol.self]))
        auth.clearAuth()
        login(auth)
        let manager = PushNotificationManager(system: TestPushSystem(), storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        await register(manager, fixture: fixture)
        let old = try XCTUnwrap(auth.captureContext())
        let oldKey = try XCTUnwrap(manager.contentKey())
        auth.clearAuth()
        login(auth, user: "bob")
        manager.configure(apiClient: APIClient(authManager: auth))
        XCTAssertNil(manager.contentKey())
        let replacement = SetupHTTPFixture()
        replacement.enable()
        SetupURLProtocol.fixture = replacement
        let registered = expectation(description: "replacement login registered")
        replacement.registered = { registered.fulfill() }
        await manager.reconcile()
        await fulfillment(of: [registered], timeout: 3)
        await manager.waitForPendingRegistrations()
        let replacementKey = try XCTUnwrap(manager.contentKey())
        XCTAssertNotEqual(replacementKey.keyId, oldKey.keyId)
        XCTAssertNotEqual(replacementKey.key, oldKey.key)
        XCTAssertEqual(replacementKey.userId, "bob")
        await manager.unregister(context: old)
        XCTAssertEqual(manager.contentKey()?.keyId, replacementKey.keyId)
        XCTAssertEqual(manager.confirmed?.registered, true)
        XCTAssertEqual(auth.user?.id, "bob")
    }

    private func register(_ manager: PushNotificationManager, fixture: SetupHTTPFixture, byte: UInt8 = 10) async {
        let registered = expectation(description: "installation registration reached Server")
        fixture.registered = { registered.fulfill() }
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: byte, count: 32))
        await fulfillment(of: [registered], timeout: 3)
        await manager.waitForPendingRegistrations()
        fixture.registered = nil
    }

    private func registrations(_ fixture: SetupHTTPFixture) -> [URLRequest] {
        fixture.snapshot().filter { $0.url?.path == "/api/mobile/push/encrypted-register" }
    }
}
