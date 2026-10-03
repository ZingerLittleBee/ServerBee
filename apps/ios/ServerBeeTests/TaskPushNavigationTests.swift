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

    private func encrypted(_ context: MobileAuthenticationContext, invalid: Bool = false, age: Int64 = 0) throws -> (PushEnvelope, PushContentKey) {
        let now = Int64(Date().timeIntervalSince1970) - age
        let summary = TaskRunPushSummary(taskId: taskId, runId: runId, total: 4, failed: invalid ? -1 : 1, timedOut: 1, offline: 1, denied: 1)
        let content = PushContent(kind: "task_failure", deploymentId: context.serverUrl, userId: context.userId, installationId: context.installationId,
                                  eventId: runId, createdAt: now, expiresAt: now + 1800, taskRun: summary)
        let key = PushContentKey(keyId: "task-key", key: Data(repeating: 7, count: 32).base64EncodedString(), deploymentId: context.serverUrl,
                                 userId: context.userId, installationId: context.installationId, scope: context.pushScope)
        let identity = try content.identity
        let aad = Data("ServerBee.Push.v1|\(key.keyId)|\(identity)".utf8)
        let sealed = try AES.GCM.seal(JSONEncoder().encode(content), using: SymmetricKey(data: try XCTUnwrap(Data(base64Encoded: key.key))), authenticating: aad)
        return (PushEnvelope(version: 1, keyId: key.keyId, identity: identity,
                             nonce: sealed.nonce.withUnsafeBytes { Data($0) }.base64EncodedString(),
                             ciphertext: (sealed.ciphertext + sealed.tag).base64EncodedString()), key)
    }

    func testEarlyDelegateTapKeepsExactRunAndRejectsStaleAccountAndMalformedSummary() throws {
        let current = context()
        let (envelope, key) = try encrypted(current)
        let delegate = AppDelegate()
        delegate.bufferNotification(userInfo: ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))])
        let router = PushNotificationRouter()
        delegate.pushRouter = router
        XCTAssertNotNil(router.pendingEnvelope)
        XCTAssertEqual(router.consumeAccountTarget(context: current, key: key), .taskRun(taskId: taskId, runId: runId))
        XCTAssertNil(router.pendingEnvelope)
        router.enqueue(envelope: envelope)
        XCTAssertNil(router.consumeAccountTarget(context: context(user: "bob"), key: key))
        let invalid = try encrypted(current, invalid: true).0
        router.enqueue(envelope: invalid)
        XCTAssertNil(router.consumeAccountTarget(context: current, key: key))
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
        }
        let auth = AuthManager()
        auth.setServerUrl("https://serverbee.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-access", accessExpiresInSecs: 900, refreshToken: "fixture-refresh",
                                                    refreshExpiresInSecs: 3600, tokenType: "Bearer", user: MobileUser(id: "alice", username: "alice", role: "admin")))
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
    func testInFlightResultsCannotAppearAfterAccountReplacement() async throws {
        URLProtocol.registerClass(PushLifecycleURLProtocol.self)
        defer {
            PushLifecycleURLProtocol.handler = nil
            PushLifecycleURLProtocol.cancelPending()
            URLProtocol.unregisterClass(PushLifecycleURLProtocol.self)
            AuthManager().clearAuth()
        }
        let auth = AuthManager()
        auth.setServerUrl("https://serverbee.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-access", accessExpiresInSecs: 900, refreshToken: "fixture-refresh",
                                                    refreshExpiresInSecs: 3600, tokenType: "Bearer", user: MobileUser(id: "alice", username: "alice", role: "admin")))
        let api = APIClient(authManager: auth)
        let target = TaskRunTarget(taskId: taskId, runId: runId)
        let entered = expectation(description: "old account request captured")
        let model = TaskRunResultsViewModel()
        let held = TaskPushHeldRequest()
        PushLifecycleURLProtocol.handler = { request in held.hold(request); entered.fulfill() }
        let loading = Task { await model.load(target: target, apiClient: api, isAdmin: true) }
        await fulfillment(of: [entered], timeout: 3)
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "replacement-access", accessExpiresInSecs: 900, refreshToken: "replacement-refresh",
                                                    refreshExpiresInSecs: 3600, tokenType: "Bearer", user: MobileUser(id: "bob", username: "bob", role: "admin")))
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
        XCTAssertEqual(try PushEnvelopeDecoder.decryptForNavigation(envelope, key: key).taskRun?.runId, runId)
        let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))
        for cold in [true, false] {
            let delegate = AppDelegate()
            let router = PushNotificationRouter()
            if !cold { delegate.pushRouter = router }
            delegate.bufferNotification(userInfo: ["serverbee_envelope": object])
            if cold { delegate.pushRouter = router }
            XCTAssertEqual(router.consumeAccountTarget(context: current, key: key), .taskRun(taskId: taskId, runId: runId))
            XCTAssertNil(router.pendingEnvelope)
        }
        let input = UNMutableNotificationContent()
        input.userInfo = ["serverbee_envelope": object]
        let rendered = PushNotificationRenderer.render(input, key: key)
        XCTAssertEqual(rendered.body, String(localized: "Open ServerBee to view this notification."))
        XCTAssertTrue(rendered.userInfo.isEmpty, "Newly rendered expired delivery must still fail closed")
        let router = PushNotificationRouter()
        router.enqueue(envelope: envelope)
        XCTAssertNil(router.consumeAccountTarget(context: context(user: "bob"), key: key))
        let tampered = PushEnvelope(version: envelope.version, keyId: envelope.keyId, identity: envelope.identity,
                                    nonce: envelope.nonce, ciphertext: Data(repeating: 0, count: 64).base64EncodedString())
        router.enqueue(envelope: tampered)
        XCTAssertNil(router.consumeAccountTarget(context: current, key: key))
    }

    func testLateTaskTapStillFetchesCurrentServerAuthorizationAndFallsBackOnRevocation() async throws {
        URLProtocol.registerClass(PushLifecycleURLProtocol.self)
        defer {
            PushLifecycleURLProtocol.handler = nil
            PushLifecycleURLProtocol.cancelPending()
            URLProtocol.unregisterClass(PushLifecycleURLProtocol.self)
            AuthManager().clearAuth()
        }
        let auth = AuthManager()
        auth.setServerUrl("https://serverbee.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-access", accessExpiresInSecs: 900, refreshToken: "fixture-refresh",
                                                    refreshExpiresInSecs: 3600, tokenType: "Bearer", user: MobileUser(id: "alice", username: "alice", role: "admin")))
        let current = try XCTUnwrap(auth.captureContext())
        let (envelope, key) = try encrypted(current, age: 3600)
        let router = PushNotificationRouter()
        router.enqueue(envelope: envelope)
        guard let link = router.consumeAccountTarget(context: current, key: key), case let .taskRun(taskId, runId) = link else {
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
