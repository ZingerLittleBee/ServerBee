import Foundation
import XCTest
@testable import ServerBee

@MainActor
final class TestPushActionTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(PushLifecycleURLProtocol.self) }
    override func tearDown() async throws {
        PushLifecycleURLProtocol.handler = nil
        PushLifecycleURLProtocol.cancelPending()
        URLProtocol.unregisterClass(PushLifecycleURLProtocol.self)
        AuthManager().clearAuth()
    }

    func testTargetedActionSendsOnlyConfirmedRevisionAndReportsProviderAcceptance() async throws {
        let auth = AuthManager()
        auth.setServerUrl("https://serverbee.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-access", accessExpiresInSecs: 900,
                                                    refreshToken: "fixture-refresh", refreshExpiresInSecs: 3600, tokenType: "Bearer",
                                                    user: MobileUser(id: "alice", username: "alice", role: "member")))
        let storage = MemoryPushSetupStorage()
        let manager = PushNotificationManager(system: TestPushSystem(), relay: TestPushRelay(), storage: storage)
        let registered = expectation(description: "content key registered securely")
        let sent = expectation(description: "test request has no caller-selected recipient")
        PushLifecycleURLProtocol.handler = { request in
            let body = PushSetupTestData.body(request.request)
            switch request.request.url?.path {
            case "/api/mobile/push/verified-register":
                XCTAssertEqual(request.request.url?.scheme, "https")
                XCTAssertEqual(Data(base64Encoded: body["content_key"] as? String ?? "")?.count, 32)
                XCTAssertNotNil(body["content_key_id"] as? String)
                registered.fulfill()
                request.respond(200)
            case "/api/mobile/push/test":
                XCTAssertEqual(Set(body.keys), ["expected_revision"])
                XCTAssertEqual((body["expected_revision"] as? NSNumber)?.int64Value, 2)
                XCTAssertEqual(request.request.value(forHTTPHeaderField: "Authorization"), "Bearer fixture-access")
                sent.fulfill()
                request.respond(200, data: Data(#"{"data":{"event_id":"11111111-1111-4111-8111-111111111111","outcome":"accepted","reason":"Accepted","presentation":"unobserved"}}"#.utf8))
            default: request.respond(200)
            }
        }
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 0xaa, count: 32))
        await fulfillment(of: [registered], timeout: 3)
        await manager.waitForPendingRegistrations()
        XCTAssertTrue(manager.confirmed?.registered == true)
        XCTAssertNotNil(storage.load(PushContentKey.storageKey))
        await manager.sendTestNotification()
        await fulfillment(of: [sent], timeout: 3)
        XCTAssertEqual(manager.testResult?.outcome, "accepted")
        XCTAssertEqual(manager.testResult?.presentation, "unobserved")
        XCTAssertFalse(manager.isTesting)
    }

    func testHttpSetupNeverTransmitsContentKey() async {
        let auth = AuthManager()
        auth.setServerUrl("http://127.0.0.1:9527")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-access", accessExpiresInSecs: 900,
                                                    refreshToken: "fixture-refresh", refreshExpiresInSecs: 3600, tokenType: "Bearer",
                                                    user: MobileUser(id: "alice", username: "alice", role: "member")))
        let relay = TestPushRelay()
        let registration = expectation(description: "HTTP content key request forbidden")
        registration.isInverted = true
        PushLifecycleURLProtocol.handler = { request in
            if request.request.url?.path == "/api/mobile/push/verified-register" { registration.fulfill() }
            request.respond(200)
        }
        let manager = PushNotificationManager(system: TestPushSystem(), relay: relay, storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 0xaa, count: 32))
        await fulfillment(of: [registration], timeout: 0.2)
        await manager.waitForPendingRegistrations()
        XCTAssertEqual(relay.attempts, 0)
        XCTAssertNotNil(manager.errorMessage)
        XCTAssertNil(manager.contentKey())
    }
}
