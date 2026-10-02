import Foundation
import XCTest
@testable import ServerBee

/// Substitutes only the external Server HTTP boundary; the manager and API client are real.
private final class DemotionHTTPFixture: @unchecked Sendable {
    private let lock = NSLock()
    private var preferences = PushPreferences(enabled: true, alerts: true, security: true, taskFailure: true, taskSuccess: false)
    private var revision: Int64 = 2
    private var securityAllowed = true
    private var saveStatus = 200
    private var attempts: [PushPreferences] = []
    private var expectedRevisions: [Int64] = []

    func demote() { lock.lock(); defer { lock.unlock() }; securityAllowed = false }
    func setSaveStatus(_ value: Int) { lock.lock(); defer { lock.unlock() }; saveStatus = value }
    func savedAttempts() -> [PushPreferences] { lock.lock(); defer { lock.unlock() }; return attempts }
    func requestedRevisions() -> [Int64] { lock.lock(); defer { lock.unlock() }; return expectedRevisions }

    func handle(_ request: URLRequest) throws -> (Int, Data) {
        lock.lock(); defer { lock.unlock() }
        guard request.url?.path == "/api/mobile/push/settings" else { throw URLError(.unsupportedURL) }
        if request.httpMethod == "PUT" {
            let body = PushSetupTestData.body(request)
            guard let expected = body["expected_revision"] as? NSNumber, let draft = body["preferences"] else {
                throw URLError(.badServerResponse)
            }
            let data = try JSONSerialization.data(withJSONObject: draft)
            let submitted = try JSONDecoder.snakeCase.decode(PushPreferences.self, from: data)
            attempts.append(submitted)
            expectedRevisions.append(expected.int64Value)
            if saveStatus != 200 { return (saveStatus, error("Save rejected")) }
            if expected.int64Value != revision { return (409, error("Stale revision")) }
            if submitted.security && !securityAllowed { return (403, error("Security notifications require an administrator")) }
            revision = expected.int64Value + 1
            preferences = submitted
        } else if request.httpMethod != "GET" { throw URLError(.unsupportedURL) }
        let encoded = try JSONEncoder.snakeCase.encode(preferences)
        guard let json = String(bytes: encoded, encoding: .utf8) else { throw URLError(.badServerResponse) }
        return (200, Data("""
        {"data":{"revision":\(revision),"preferences":\(json),"security_allowed":\(securityAllowed),
        "registered":\(preferences.enabled),"grant_expires_at":\(preferences.enabled ? "\"2033-05-18T03:33:20Z\"" : "null"),
        "relay_url":"https://relay.test","delivery_available":false}}
        """.utf8))
    }

    private func error(_ message: String) -> Data { Data("{\"error\":{\"message\":\"\(message)\"}}".utf8) }
}

private final class DemotionURLProtocol: URLProtocol {
    nonisolated(unsafe) static var fixture: DemotionHTTPFixture?
    private static let pending = PendingURLProtocolRequests()
    override static func canInit(with request: URLRequest) -> Bool { true }
    override static func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func stopLoading() { _ = Self.pending.finish(self) }
    static func cancelPending() { pending.cancelAll() }
    override func startLoading() {
        Self.pending.begin(self)
        do {
            guard let fixture = Self.fixture, let url = request.url else { throw URLError(.cancelled) }
            let (status, data) = try fixture.handle(request)
            guard let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: nil, headerFields: nil) else {
                throw URLError(.badServerResponse)
            }
            guard Self.pending.finish(self) else { return }
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch {
            if Self.pending.finish(self) { client?.urlProtocol(self, didFailWithError: error) }
        }
    }
}

@MainActor
final class NotificationSetupDemotionTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(DemotionURLProtocol.self) }
    override func tearDown() async throws {
        DemotionURLProtocol.fixture = nil
        DemotionURLProtocol.cancelPending()
        URLProtocol.unregisterClass(DemotionURLProtocol.self)
        AuthManager().clearAuth()
    }

    private func login(_ auth: AuthManager) {
        auth.setServerUrl("https://demotion.test")
        auth.handleLoginResponse(MobileTokenResponse(
            accessToken: "access-\(UUID().uuidString)", accessExpiresInSecs: 900,
            refreshToken: "refresh-\(UUID().uuidString)", refreshExpiresInSecs: 3600,
            tokenType: "Bearer", user: MobileUser(id: "alice", username: "alice", role: "admin"),
            revocationToken: "proof-\(UUID().uuidString)"
        ))
    }

    func testDemotionAllowsActualSaveAndDisableDespiteHiddenOrStaleAdminDraft() async throws {
        for cachedRole in ["admin", "member"] {
            for enabled in [true, false] {
                let http = DemotionHTTPFixture()
                DemotionURLProtocol.fixture = http
                let auth = AuthManager()
                login(auth)
                let system = TestPushSystem()
                system.status = .denied
                let relay = TestPushRelay()
                let manager = PushNotificationManager(system: system, relay: relay, storage: MemoryPushSetupStorage())
                manager.configure(apiClient: APIClient(authManager: auth))
                await manager.reconcile()
                XCTAssertEqual(manager.confirmed?.securityAllowed, true)
                var draft = try XCTUnwrap(manager.confirmed?.preferences)
                XCTAssertTrue(draft.security)
                http.demote()
                auth.user = MobileUser(id: "alice", username: "alice", role: cachedRole)
                await manager.reconcile()
                XCTAssertEqual(auth.user?.role, cachedRole)
                XCTAssertEqual(manager.confirmed?.securityAllowed, false)
                XCTAssertEqual(manager.confirmed?.revision, 2, "Role changes do not change registration revision")
                XCTAssertEqual(manager.confirmed?.preferences.security, true, "Stored intent is not optimistically rewritten")
                draft.enabled = enabled
                draft.alerts = false
                draft.taskSuccess = true
                // These are the view's Save/Disable action entry points, with the old hidden draft intact.
                await manager.savePreferences(draft)
                var permitted = draft
                permitted.security = false
                XCTAssertEqual(http.savedAttempts(), [permitted])
                XCTAssertEqual(http.requestedRevisions(), [2])
                XCTAssertEqual(manager.confirmed?.preferences, permitted)
                XCTAssertEqual(manager.confirmed?.revision, 3)
                XCTAssertEqual(manager.confirmed?.securityAllowed, false)
                XCTAssertEqual(manager.confirmed?.registered, enabled)
                XCTAssertEqual(manager.confirmed?.deliveryAvailable, false)
                XCTAssertNil(manager.errorMessage)
                XCTAssertEqual(system.permissionRequests, 0)
                XCTAssertEqual(relay.attempts, 0)
                await manager.waitForPendingRegistrations()
                auth.clearAuth()
            }
        }
    }

    func testDemotionBetweenReadAndSaveFailsHonestlyThenRecoversAfterServerConfirmation() async throws {
        let http = DemotionHTTPFixture()
        DemotionURLProtocol.fixture = http
        let auth = AuthManager()
        login(auth)
        let system = TestPushSystem()
        system.status = .denied
        let relay = TestPushRelay()
        let manager = PushNotificationManager(system: system, relay: relay, storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        var disabled = try XCTUnwrap(manager.confirmed?.preferences)
        disabled.enabled = false
        http.demote() // The last confirmed permission is now stale; Server still rejects security=true.
        await manager.savePreferences(disabled)
        XCTAssertNotNil(manager.errorMessage)
        XCTAssertEqual(manager.confirmed?.revision, 2)
        XCTAssertEqual(manager.confirmed?.preferences.enabled, true)
        XCTAssertEqual(manager.confirmed?.preferences.security, true)
        XCTAssertEqual(manager.confirmed?.registered, true)
        XCTAssertEqual(http.savedAttempts().map { $0.security }, [true])
        await manager.reconcile()
        XCTAssertEqual(manager.confirmed?.securityAllowed, false)
        XCTAssertEqual(manager.confirmed?.revision, 2)
        http.setSaveStatus(503)
        await manager.savePreferences(disabled)
        XCTAssertNotNil(manager.errorMessage)
        XCTAssertEqual(manager.confirmed?.revision, 2)
        XCTAssertEqual(manager.confirmed?.preferences.enabled, true)
        XCTAssertEqual(manager.confirmed?.preferences.security, true)
        XCTAssertEqual(manager.confirmed?.registered, true)
        http.setSaveStatus(200)
        await manager.savePreferences(disabled)
        XCTAssertNil(manager.errorMessage)
        XCTAssertEqual(manager.confirmed?.revision, 3)
        XCTAssertEqual(manager.confirmed?.preferences.enabled, false)
        XCTAssertEqual(manager.confirmed?.preferences.security, false)
        XCTAssertEqual(manager.confirmed?.registered, false)
        XCTAssertEqual(http.savedAttempts().map { $0.security }, [true, false, false])
        XCTAssertEqual(http.requestedRevisions(), [2, 2, 2])
        XCTAssertEqual(system.permissionRequests, 0)
        XCTAssertEqual(relay.attempts, 0)
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertEqual(auth.user?.role, "admin", "Cached login metadata is not the category authority")
        await manager.waitForPendingRegistrations()
    }
}
