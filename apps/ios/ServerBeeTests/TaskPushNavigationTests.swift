import CryptoKit
import Foundation
import UserNotifications
import XCTest
@testable import ServerBee

@MainActor
final class TaskPushNavigationTests: XCTestCase {
    private let taskId = "11111111-1111-4111-8111-111111111111"
    private let runId = "22222222-2222-4222-8222-222222222222"

    private func context(user: String = "alice") -> MobileAuthenticationContext {
        MobileAuthenticationContext(serverUrl: "https://serverbee.test", userId: user, installationId: "task-install",
                                    generation: UUID(), accessToken: "fixture-access", revocationToken: "fixture-proof", refreshToken: "fixture-refresh")
    }

    private func encrypted(_ context: MobileAuthenticationContext, invalid: Bool = false, age: Int64 = 0,
                           success: Bool = false, kind: String? = nil, total: Int = 4, mixedTarget: Bool = false) throws -> (PushEnvelope, PushContentKey) {
        let now = Int64(Date().timeIntervalSince1970) - age
        let summary = TaskRunPushSummary(taskId: taskId, runId: runId, total: total, failed: invalid ? -1 : (success ? 0 : 1),
                                         timedOut: success ? 0 : 1, offline: success ? 0 : 1, denied: success ? 0 : 1)
        let targetParts = ["rule-1", "server-1", "", "2026-10-03T00:00:00+00:00"]
        let alertKey = "v1." + (try JSONEncoder().encode(targetParts)).base64EncodedString()
            .replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
        let alert = AlertPushTarget(alertKey: alertKey, status: "firing", ruleName: "Alert", serverName: "Server")
        XCTAssertTrue(alert.isValid)
        let content = PushContent(kind: kind ?? (success ? "task_success" : "task_failure"),
                                  deploymentId: context.serverUrl, userId: context.userId, installationId: context.installationId,
                                  eventId: runId, createdAt: now, expiresAt: now + 1800, taskRun: summary,
                                  alert: mixedTarget ? alert : nil)
        let key = PushContentKey(keyId: "task-key", key: Data(repeating: 7, count: 32).base64EncodedString(), deploymentId: context.serverUrl,
                                 userId: context.userId, installationId: context.installationId, scope: context.pushScope)
        let identity = try content.identity
        let aad = Data("ServerBee.Push.v1|\(key.keyId)|\(identity)".utf8)
        let sealed = try AES.GCM.seal(JSONEncoder().encode(content), using: SymmetricKey(data: try XCTUnwrap(Data(base64Encoded: key.key))), authenticating: aad)
        return (PushEnvelope(version: 1, keyId: key.keyId, identity: identity,
                             nonce: sealed.nonce.withUnsafeBytes { Data($0) }.base64EncodedString(),
                             ciphertext: (sealed.ciphertext + sealed.tag).base64EncodedString()), key)
    }

    func testTaskCategoryRejectsMixedAlertTargetsForDeliveryAndLateTaps() throws {
        let current = context()
        for success in [false, true] {
            let (envelope, key) = try encrypted(current, age: 3600, success: success, mixedTarget: true)
            XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(envelope, key: key))
            XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(envelope, key: key, purpose: .notificationTap))
            let router = PushNotificationRouter()
            router.enqueue(envelope: envelope)
            XCTAssertNil(router.consumeTarget(context: current, key: key))
        }
    }

    func testEarlyDelegateTapKeepsExactRunAndRejectsStaleAccountAndMalformedSummary() throws {
        let current = context()
        let (envelope, key) = try encrypted(current)
        let delegate = AppDelegate()
        delegate.bufferNotification(userInfo: ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))])
        let router = PushNotificationRouter()
        delegate.pushRouter = router
        XCTAssertNotNil(router.pendingEnvelope)
        XCTAssertEqual(router.consumeTarget(context: current, key: key), .taskRun(taskId: taskId, runId: runId))
        XCTAssertNil(router.pendingEnvelope)
        router.enqueue(envelope: envelope)
        XCTAssertNil(router.consumeTarget(context: context(user: "bob"), key: key))
        let invalid = try encrypted(current, invalid: true).0
        router.enqueue(envelope: invalid)
        XCTAssertNil(router.consumeTarget(context: current, key: key))
        var tab = 0
        var servers: [ServerNavigationTarget] = [.detailById("old")]
        var alerts: [ServerDeepLink] = [.alertDetail(alertKey: "old")]
        ContentView.applyDeepLink(.taskRun(taskId: taskId, runId: runId), selectedTab: &tab, serversPath: &servers, alertsPath: &alerts)
        XCTAssertEqual(tab, 3)
        XCTAssertTrue(servers.isEmpty)
        XCTAssertTrue(alerts.isEmpty)
    }

    func testExtensionRendersOnlyCountsAndRetainsAuthenticatedRunEnvelope() throws {
        let (envelope, key) = try encrypted(context())
        let input = UNMutableNotificationContent()
        input.body = "untrusted-command-output"
        input.userInfo = ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))]
        let rendered = PushNotificationRenderer.render(input, key: key)
        XCTAssertEqual(rendered.title, String(localized: "Task run failed"))
        XCTAssertFalse(rendered.body.contains(input.body))
        XCTAssertNotNil(rendered.userInfo["serverbee_envelope"])
        let fallback = PushNotificationRenderer.render(input, key: nil)
        XCTAssertEqual(fallback.body, String(localized: "Open ServerBee to view this notification."))
        XCTAssertTrue(fallback.userInfo.isEmpty)
    }

    func testResultsRequestFiltersRunAndDeletedForbiddenOrMismatchedRowsFallback() async throws {
        URLProtocol.registerClass(PushLifecycleURLProtocol.self)
        defer {
            PushLifecycleURLProtocol.handler = nil
            URLProtocol.unregisterClass(PushLifecycleURLProtocol.self)
            AuthManager().clearAuth()
            try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
        }
        let auth = AuthManager()
        auth.setServerUrl("https://serverbee.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-access", accessExpiresInSecs: 900, refreshToken: "fixture-refresh",
                                                    refreshExpiresInSecs: 3600, tokenType: "Bearer", user: MobileUser(id: "alice", username: "alice", role: "admin"),
                                                    revocationToken: "fixture-deletion-" + UUID().uuidString, mobileSessionId: UUID().uuidString))
        let target = TaskRunTarget(taskId: taskId, runId: runId)
        let model = TaskRunResultsViewModel()
        let api = APIClient(authManager: auth)
        for status in [200, 403, 404] {
            PushLifecycleURLProtocol.handler = { request in
                XCTAssertEqual(request.request.url?.path, "/api/tasks/\(target.taskId)/results")
                XCTAssertEqual(request.request.url.flatMap { URLComponents(url: $0, resolvingAgainstBaseURL: false) }?.queryItems?.first?.value, target.runId)
                let data = Data("""
                {"data":[{"id":1,"task_id":"\(target.taskId)","server_id":"server","output":"authenticated-output",
                "exit_code":1,"run_id":"\(target.runId)","attempt":1,"finished_at":"2026-10-03T00:00:00Z"}]}
                """.utf8)
                request.respond(status, data: data)
            }
            await model.load(target: target, apiClient: api, isAdmin: true)
            XCTAssertEqual(model.unavailable, status != 200)
            XCTAssertEqual(model.results.count, status == 200 ? 1 : 0)
        }
        await model.load(target: target, apiClient: api, isAdmin: false)
        XCTAssertTrue(model.unavailable)
        XCTAssertTrue(model.results.isEmpty)
        PushLifecycleURLProtocol.handler = { request in
            request.respond(200, data: Data("""
            {"data":[{"id":1,"task_id":"\(target.taskId)","server_id":"server","output":"wrong-run-output",
            "exit_code":1,"run_id":"33333333-3333-4333-8333-333333333333","attempt":1,"finished_at":"2026-10-03T00:00:00Z"}]}
            """.utf8))
        }
        await model.load(target: target, apiClient: api, isAdmin: true)
        XCTAssertTrue(model.unavailable)
        XCTAssertTrue(model.results.isEmpty)
    }
}

extension TaskPushNavigationTests {
    func testSuccessUsesActualExtensionAndEarlyAuthenticatedExactRunNavigation() throws {
        let current = context()
        let (envelope, key) = try encrypted(current, success: true)
        let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))
        let input = UNMutableNotificationContent()
        input.body = "untrusted-command-output"
        input.userInfo = ["serverbee_envelope": object]
        let service = NotificationService()
        service.loadKey = { key }
        var completions = 0
        service.didReceive(UNNotificationRequest(identifier: "success", content: input, trigger: nil)) { rendered in
            completions += 1
            XCTAssertEqual(rendered.title, String(localized: "Task run succeeded"))
            XCTAssertEqual(rendered.body, String(format: String(localized: "All %lld targets succeeded."), Int64(4)))
            XCTAssertFalse(rendered.body.contains(input.body))
            XCTAssertNotNil(rendered.userInfo["serverbee_envelope"])
        }
        service.serviceExtensionTimeWillExpire()
        XCTAssertEqual(completions, 1)
        let delegate = AppDelegate()
        delegate.bufferNotification(userInfo: ["serverbee_envelope": object])
        let router = PushNotificationRouter()
        delegate.pushRouter = router
        XCTAssertEqual(router.consumeTarget(context: current, key: key), .taskRun(taskId: taskId, runId: runId))
        router.enqueue(envelope: envelope)
        XCTAssertNil(router.consumeTarget(context: context(user: "bob"), key: key))
    }

    func testSuccessRejectsFailureCountsEmptyTargetsAndKindMismatch() throws {
        let current = context()
        let candidates = [
            try encrypted(current, kind: "task_success"),
            try encrypted(current, success: true, kind: "task_failure"),
            try encrypted(current, success: true, total: 0),
            try encrypted(current, invalid: true, success: true)
        ]
        for (envelope, key) in candidates {
            XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(envelope, key: key))
            let router = PushNotificationRouter()
            router.enqueue(envelope: envelope)
            XCTAssertNil(router.consumeTarget(context: current, key: key))
            let input = UNMutableNotificationContent()
            input.userInfo = ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))]
            let rendered = PushNotificationRenderer.render(input, key: key)
            XCTAssertEqual(rendered.body, String(localized: "Open ServerBee to view this notification."))
            XCTAssertTrue(rendered.userInfo.isEmpty)
        }
    }

    func testLateSuccessTapKeepsRunButExpiredNewDeliveryRemainsGeneric() throws {
        let current = context()
        let (envelope, key) = try encrypted(current, age: 3600, success: true)
        XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(envelope, key: key))
        let router = PushNotificationRouter()
        router.enqueue(envelope: envelope)
        XCTAssertEqual(router.consumeTarget(context: current, key: key), .taskRun(taskId: taskId, runId: runId))
        let input = UNMutableNotificationContent()
        input.userInfo = ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))]
        XCTAssertTrue(PushNotificationRenderer.render(input, key: key).userInfo.isEmpty)
    }

    func testInFlightResultsCannotAppearAfterAccountReplacement() async throws {
        URLProtocol.registerClass(PushLifecycleURLProtocol.self)
        defer {
            PushLifecycleURLProtocol.handler = nil
            PushLifecycleURLProtocol.cancelPending()
            URLProtocol.unregisterClass(PushLifecycleURLProtocol.self)
            AuthManager().clearAuth()
            try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
        }
        let auth = AuthManager()
        auth.setServerUrl("https://serverbee.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-access", accessExpiresInSecs: 900, refreshToken: "fixture-refresh",
                                                    refreshExpiresInSecs: 3600, tokenType: "Bearer", user: MobileUser(id: "alice", username: "alice", role: "admin"),
                                                    revocationToken: "fixture-deletion-" + UUID().uuidString, mobileSessionId: UUID().uuidString))
        let api = APIClient(authManager: auth)
        let target = TaskRunTarget(taskId: taskId, runId: runId)
        let entered = expectation(description: "old account request captured")
        let model = TaskRunResultsViewModel()
        let held = TaskPushHeldRequest()
        PushLifecycleURLProtocol.handler = { request in held.hold(request); entered.fulfill() }
        let loading = Task { await model.load(target: target, apiClient: api, isAdmin: true) }
        await fulfillment(of: [entered], timeout: 3)
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "replacement-access", accessExpiresInSecs: 900, refreshToken: "replacement-refresh",
                                                    refreshExpiresInSecs: 3600, tokenType: "Bearer", user: MobileUser(id: "bob", username: "bob", role: "admin"),
                                                    revocationToken: "fixture-deletion-" + UUID().uuidString, mobileSessionId: UUID().uuidString))
        held.release(data: Data("{\"data\":[]}".utf8))
        await loading.value
        XCTAssertTrue(model.unavailable)
        XCTAssertTrue(model.results.isEmpty)
    }
}

private final class TaskPushHeldRequest: @unchecked Sendable {
    private let lock = NSLock()
    private var request: PushLifecycleURLProtocol?
    func hold(_ value: PushLifecycleURLProtocol) {
        lock.lock(); defer { lock.unlock() }
        request = value
    }
    func release(data: Data) {
        lock.lock()
        let value = request
        request = nil
        lock.unlock()
        value?.respond(200, data: data)
    }
}

extension TaskPushNavigationTests {
    func testLateColdAndWarmTapsKeepExactRunWhileRenderingStaysExpired() throws {
        let current = context()
        let (envelope, key) = try encrypted(current, age: 3600)
        XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(envelope, key: key))
        XCTAssertEqual(try PushEnvelopeDecoder.decrypt(envelope, key: key, purpose: .notificationTap).taskRun?.runId, runId)
        let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))
        for cold in [true, false] {
            let delegate = AppDelegate()
            let router = PushNotificationRouter()
            if !cold { delegate.pushRouter = router }
            delegate.bufferNotification(userInfo: ["serverbee_envelope": object])
            if cold { delegate.pushRouter = router }
            XCTAssertEqual(router.consumeTarget(context: current, key: key), .taskRun(taskId: taskId, runId: runId))
            XCTAssertNil(router.pendingEnvelope)
        }
        let input = UNMutableNotificationContent()
        input.userInfo = ["serverbee_envelope": object]
        let rendered = PushNotificationRenderer.render(input, key: key)
        XCTAssertEqual(rendered.body, String(localized: "Open ServerBee to view this notification."))
        XCTAssertTrue(rendered.userInfo.isEmpty, "Newly rendered expired delivery must still fail closed")
        let router = PushNotificationRouter()
        router.enqueue(envelope: envelope)
        XCTAssertNil(router.consumeTarget(context: context(user: "bob"), key: key))
        let tampered = PushEnvelope(version: envelope.version, keyId: envelope.keyId, identity: envelope.identity,
                                    nonce: envelope.nonce, ciphertext: Data(repeating: 0, count: 64).base64EncodedString())
        router.enqueue(envelope: tampered)
        XCTAssertNil(router.consumeTarget(context: current, key: key))
    }

    func testLateTaskTapStillFetchesCurrentServerAuthorizationAndFallsBackOnRevocation() async throws {
        URLProtocol.registerClass(PushLifecycleURLProtocol.self)
        defer {
            PushLifecycleURLProtocol.handler = nil
            PushLifecycleURLProtocol.cancelPending()
            URLProtocol.unregisterClass(PushLifecycleURLProtocol.self)
            AuthManager().clearAuth()
            try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
        }
        let auth = AuthManager()
        auth.setServerUrl("https://serverbee.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-access", accessExpiresInSecs: 900, refreshToken: "fixture-refresh",
                                                    refreshExpiresInSecs: 3600, tokenType: "Bearer", user: MobileUser(id: "alice", username: "alice", role: "admin"),
                                                    revocationToken: "fixture-deletion-" + UUID().uuidString, mobileSessionId: UUID().uuidString))
        let current = try XCTUnwrap(auth.captureContext())
        let (envelope, key) = try encrypted(current, age: 3600)
        let router = PushNotificationRouter()
        router.enqueue(envelope: envelope)
        guard let link = router.consumeTarget(context: current, key: key), case let .taskRun(taskId, runId) = link else {
            XCTFail("Late tap must retain the run until Server authorization"); return
        }
        let requested = expectation(description: "late tap performs current authenticated exact-run read")
        let target = TaskRunTarget(taskId: taskId, runId: runId)
        PushLifecycleURLProtocol.handler = { request in
            XCTAssertEqual(request.request.url?.path, "/api/tasks/\(target.taskId)/results")
            XCTAssertEqual(request.request.url.flatMap { URLComponents(url: $0, resolvingAgainstBaseURL: false) }?.queryItems?.first?.value, target.runId)
            XCTAssertEqual(request.request.value(forHTTPHeaderField: "Authorization"), "Bearer fixture-access")
            requested.fulfill()
            request.respond(403)
        }
        let model = TaskRunResultsViewModel()
        await model.load(target: target, apiClient: APIClient(authManager: auth), isAdmin: true)
        await fulfillment(of: [requested], timeout: 3)
        XCTAssertTrue(model.unavailable)
        XCTAssertTrue(model.results.isEmpty)
    }
}
