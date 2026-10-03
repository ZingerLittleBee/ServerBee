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
        try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
    }

    func testTargetedActionSendsOnlyConfirmedRevisionAndReportsProviderAcceptance() async throws {
        let auth = AuthManager()
        auth.setServerUrl("https://serverbee.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-access", accessExpiresInSecs: 900,
                                                    refreshToken: "fixture-refresh", refreshExpiresInSecs: 3600, tokenType: "Bearer",
                                                    user: MobileUser(id: "alice", username: "alice", role: "member"),
                                                    revocationToken: "fixture-deletion-" + UUID().uuidString, mobileSessionId: UUID().uuidString))
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
                XCTAssertEqual(Set(body.keys), ["expected_revision", "event_id"])
                XCTAssertNotNil(UUID(uuidString: body["event_id"] as? String ?? ""))
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

    func testLostResponseReusesIdentityAndPendingStatusSurvivesManagerRestart() async throws {
        let auth = AuthManager()
        auth.setServerUrl("https://serverbee.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-access", accessExpiresInSecs: 900,
                                                    refreshToken: "fixture-refresh", refreshExpiresInSecs: 3600, tokenType: "Bearer",
                                                    user: MobileUser(id: "alice", username: "alice", role: "member"),
                                                    revocationToken: "fixture-deletion-" + UUID().uuidString, mobileSessionId: UUID().uuidString))
        let storage = MemoryPushSetupStorage()
        let manager = PushNotificationManager(system: TestPushSystem(), relay: TestPushRelay(), storage: storage)
        let api = APIClient(authManager: auth)
        let posts = AuthenticationRequestLog()
        let reads = AuthenticationRequestLog()
        PushLifecycleURLProtocol.handler = { request in
            let path = request.request.url?.path ?? ""
            if path == "/api/mobile/push/test" {
                let count = posts.append(request.request)
                if count == 1 { request.respond(503); return }
                let event = PushSetupTestData.body(request.request)["event_id"] as? String ?? ""
                request.respond(200, data: Data("{\"data\":{\"event_id\":\"\(event)\",\"outcome\":\"pending\",\"reason\":\"Queued\",\"presentation\":\"unobserved\"}}".utf8))
            } else if path.hasPrefix("/api/mobile/push/test/") {
                let count = reads.append(request.request)
                if count == 1 { request.respond(404); return }
                let outcome = ["retryable", "accepted", "permanent", "expired"][min(count - 2, 3)]
                let event = request.request.url?.lastPathComponent ?? ""
                request.respond(200, data: Data("{\"data\":{\"event_id\":\"\(event)\",\"outcome\":\"\(outcome)\",\"reason\":\"Fixture\",\"presentation\":\"unobserved\"}}".utf8))
            } else { request.respond(200) }
        }
        manager.configure(apiClient: api)
        await manager.reconcile()
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 0xaa, count: 32))
        // Wait for the real manager's asynchronous HTTP registration boundary.
        for _ in 0..<100 {
            if manager.confirmed?.registered == true { break }
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTAssertTrue(manager.confirmed?.registered == true)
        await manager.sendTestNotification()
        XCTAssertNil(manager.testResult)
        await manager.sendTestNotification()
        XCTAssertEqual(manager.testResult?.outcome, "pending")
        XCTAssertTrue(manager.testResult?.isPending == true)
        let requests = posts.snapshot()
        XCTAssertEqual(requests.count, 2)
        XCTAssertEqual(PushSetupTestData.body(requests[0])["event_id"] as? String,
                       PushSetupTestData.body(requests[1])["event_id"] as? String)
        let restored = PushNotificationManager(system: TestPushSystem(), relay: TestPushRelay(), storage: storage)
        restored.configure(apiClient: api)
        await restored.refreshTestStatus()
        XCTAssertEqual(restored.testResult?.outcome, "retryable")
        XCTAssertTrue(restored.testResult?.isPending == true)
        XCTAssertEqual(restored.testResult?.presentation, "unobserved")
        await restored.refreshTestStatus()
        XCTAssertEqual(restored.testResult?.outcome, "accepted")
        XCTAssertFalse(restored.testResult?.isPending == true)
        await restored.refreshTestStatus()
        XCTAssertEqual(restored.testResult?.outcome, "permanent")
        await restored.refreshTestStatus()
        XCTAssertEqual(restored.testResult?.outcome, "expired")
        XCTAssertFalse(restored.testResult?.isPending == true)
    }

    func testHttpSetupNeverTransmitsContentKey() async {
        let auth = AuthManager()
        auth.setServerUrl("http://127.0.0.1:9527")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-access", accessExpiresInSecs: 900,
                                                    refreshToken: "fixture-refresh", refreshExpiresInSecs: 3600, tokenType: "Bearer",
                                                    user: MobileUser(id: "alice", username: "alice", role: "member"),
                                                    revocationToken: "fixture-deletion-" + UUID().uuidString, mobileSessionId: UUID().uuidString))
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

    func testRejectedRedirectKeepsContentKeyAndGrantForExplicitSetupRetry() async throws {
        let auth = AuthManager()
        auth.setServerUrl("https://serverbee.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-access", accessExpiresInSecs: 900,
                                                    refreshToken: "fixture-refresh", refreshExpiresInSecs: 3600, tokenType: "Bearer",
                                                    user: MobileUser(id: "alice", username: "alice", role: "member"),
                                                    revocationToken: "fixture-deletion-" + UUID().uuidString, mobileSessionId: UUID().uuidString))
        let storage = MemoryPushSetupStorage()
        let relay = TestPushRelay()
        let manager = PushNotificationManager(system: TestPushSystem(), relay: relay, storage: storage)
        let log = AuthenticationRequestLog()
        let rejected = expectation(description: "registration redirect rejected")
        let retried = expectation(description: "explicit setup retry registers at original endpoint")
        PushLifecycleURLProtocol.handler = { request in
            if request.request.url?.path == "/api/mobile/push/verified-register" {
                let count = log.append(request.request)
                if count == 1 { rejected.fulfill(); request.respond(308) } else { retried.fulfill(); request.respond(200) }
            } else { request.respond(200) }
        }
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 0xaa, count: 32))
        await fulfillment(of: [rejected], timeout: 3)
        await manager.waitForPendingRegistrations()
        XCTAssertFalse(manager.confirmed?.registered == true)
        XCTAssertNotNil(manager.errorMessage)
        let content = try XCTUnwrap(storage.load(PushContentKey.storageKey))
        await manager.retry()
        await fulfillment(of: [retried], timeout: 3)
        await manager.waitForPendingRegistrations()
        XCTAssertTrue(manager.confirmed?.registered == true)
        XCTAssertEqual(storage.load(PushContentKey.storageKey), content)
        XCTAssertEqual(relay.attempts, 1)
        let requests = log.snapshot()
        XCTAssertEqual(requests.count, 2)
        XCTAssertTrue(requests.allSatisfy { $0.url?.absoluteString == "https://serverbee.test/api/mobile/push/verified-register" })
        XCTAssertEqual(PushSetupTestData.body(requests[0])["content_key"] as? String,
                       PushSetupTestData.body(requests[1])["content_key"] as? String)
        XCTAssertEqual(PushSetupTestData.body(requests[0])["grant_token"] as? String,
                       PushSetupTestData.body(requests[1])["grant_token"] as? String)
    }

    func testDirectRegistrationCallerCannotBypassHttpsRequirement() async throws {
        let auth = AuthManager()
        auth.setServerUrl("http://serverbee.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-access", accessExpiresInSecs: 900,
                                                    refreshToken: "fixture-refresh", refreshExpiresInSecs: 3600, tokenType: "Bearer",
                                                    user: MobileUser(id: "alice", username: "alice", role: "member"),
                                                    revocationToken: "fixture-deletion-" + UUID().uuidString, mobileSessionId: UUID().uuidString))
        let context = try XCTUnwrap(auth.captureContext())
        let forbidden = expectation(description: "no content-bearing HTTP request")
        forbidden.isInverted = true
        PushLifecycleURLProtocol.handler = { request in forbidden.fulfill(); request.respond(200) }
        do {
            let _: PushSetup = try await APIClient(authManager: auth).send(
                "/api/mobile/push/verified-register", method: "POST",
                body: VerifiedPushRequest(expectedRevision: 1, deviceToken: String(repeating: "a", count: 64), environment: "sandbox",
                                          keyId: "fixture-key", grantId: "fixture-grant", grantToken: "fixture-secret",
                                          contentKeyId: UUID().uuidString, contentKey: Data(repeating: 0x41, count: 32).base64EncodedString(),
                                          deploymentId: context.serverUrl), context: context
            )
            XCTFail("Direct registration over HTTP must fail before sending")
        } catch PushSetupError.insecureServer { }
        await fulfillment(of: [forbidden], timeout: 0.2)
        XCTAssertTrue(auth.isCurrent(context))
    }
}
