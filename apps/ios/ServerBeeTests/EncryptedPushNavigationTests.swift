import CryptoKit
import Foundation
import UserNotifications
import XCTest
@testable import ServerBee

@MainActor
final class EncryptedPushNavigationTests: XCTestCase {
    private func context(user: String = "alice", deployment: String = "https://serverbee.test",
                         installation: String = "test-install", proof: String = "fixture-proof") -> MobileAuthenticationContext {
        MobileAuthenticationContext(serverUrl: deployment, userId: user, installationId: installation, generation: UUID(),
                                    accessToken: "fixture-access", revocationToken: proof, refreshToken: "fixture-refresh")
    }
    private func encrypted(_ context: MobileAuthenticationContext, kind: String = "test",
                           createdAt: Int64 = Int64(Date().timeIntervalSince1970), age: Int64 = 0) throws -> (PushEnvelope, PushContentKey) {
        let createdAt = createdAt - age
        var content = PushContent(kind: kind, deploymentId: context.serverUrl, userId: context.userId, installationId: context.installationId,
                                  eventId: UUID().uuidString.lowercased(), createdAt: createdAt, expiresAt: createdAt + 1800)
        if kind == "security" {
            content.serverId = "33333333-3333-4333-8333-333333333333"
            content.securityEventId = content.eventId
            content.securityEventType = "ssh_brute_force"
        }
        let secret = SymmetricKey(size: .bits256).withUnsafeBytes { Data($0) }
        let key = PushContentKey(keyId: UUID().uuidString.lowercased(), key: secret.base64EncodedString(),
                                 deploymentId: context.serverUrl, userId: context.userId, installationId: context.installationId, scope: context.pushScope)
        let identity = try content.identity
        let aad = Data("ServerBee.Push.v1|\(key.keyId)|\(identity)".utf8)
        let sealed = try AES.GCM.seal(JSONEncoder().encode(content), using: SymmetricKey(data: try XCTUnwrap(Data(base64Encoded: key.key))), authenticating: aad)
        let nonce = sealed.nonce.withUnsafeBytes { Data($0) }
        return (PushEnvelope(version: 1, keyId: key.keyId, identity: identity, nonce: nonce.base64EncodedString(),
                             ciphertext: (sealed.ciphertext + sealed.tag).base64EncodedString()), key)
    }

    func testEarlyDelegateTapWaitsForStoresAndRejectsReplacementAccountDeploymentAndLogin() throws {
        let current = context()
        let (envelope, key) = try encrypted(current)
        let delegate = AppDelegate()
        let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))
        delegate.bufferNotification(userInfo: ["serverbee_envelope": object])
        let router = PushNotificationRouter()
        delegate.pushRouter = router
        XCTAssertNotNil(router.pendingEnvelope)
        XCTAssertEqual(router.consumeTarget(context: current, key: key), .account)
        XCTAssertNil(router.pendingEnvelope)
        for mismatch in [context(user: "bob"), context(deployment: "https://other.test"),
                         MobileAuthenticationContext(serverUrl: current.serverUrl, userId: current.userId, installationId: current.installationId,
                                                     generation: UUID(), accessToken: "new", revocationToken: "new-login-proof", refreshToken: "new-refresh")] {
            router.enqueue(envelope: envelope)
            XCTAssertNil(router.consumeTarget(context: mismatch, key: key))
        }
        router.enqueue(envelope: envelope)
        XCTAssertNil(router.consumeTarget(context: current, key: nil))
        delegate.bufferNotification(userInfo: ["server_id": "victim", "serverbee_target": ["user_id": "alice"]])
        XCTAssertEqual(router.pendingEnvelope?.ciphertext, envelope.ciphertext, "malformed callback must not erase a tap waiting for its key")
        XCTAssertEqual(router.consumeTarget(context: current, key: key), .account)
        XCTAssertNil(router.pendingEnvelope)
    }

    func testEarlyDelegateTapWaitsForStoresAndCurrentKey() throws {
        let current = context()
        let (envelope, key) = try encrypted(current)
        let delegate = AppDelegate()
        let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))
        delegate.bufferNotification(userInfo: ["serverbee_envelope": object])
        let router = PushNotificationRouter()
        delegate.pushRouter = router
        XCTAssertEqual(router.pendingEnvelope?.ciphertext, envelope.ciphertext)
        XCTAssertNil(router.consumeTarget(context: current, key: nil))
        XCTAssertEqual(router.pendingEnvelope?.ciphertext, envelope.ciphertext)
        XCTAssertEqual(router.consumeTarget(context: current, key: key), .account)
        XCTAssertNil(router.pendingEnvelope)
    }

    func testMalformedCallbacksNeitherCreateNorReplaceBufferedTap() throws {
        let current = context()
        let (envelope, key) = try encrypted(current, kind: "security")
        let content = try PushEnvelopeDecoder.decrypt(envelope, key: key)
        let delegate = AppDelegate()
        let router = PushNotificationRouter()
        delegate.pushRouter = router
        let callbacks: [[AnyHashable: Any]] = [
            ["server_id": "victim", "serverbee_target": ["user_id": "alice"]],
            ["serverbee_envelope": "not a JSON object"],
            ["serverbee_envelope": ["version": 1]],
            ["serverbee_envelope": ["ciphertext": String(repeating: "A", count: 5000)]]
        ]
        for callback in callbacks {
            delegate.bufferNotification(userInfo: callback)
            XCTAssertNil(router.pendingEnvelope)
        }
        delegate.bufferNotification(userInfo: ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))])
        XCTAssertNil(router.consumeTarget(context: current, key: nil))
        for callback in callbacks {
            delegate.bufferNotification(userInfo: callback)
            XCTAssertEqual(router.pendingEnvelope?.ciphertext, envelope.ciphertext)
        }
        XCTAssertEqual(router.consumeTarget(context: current, key: key),
                       .securityDetail(serverId: try XCTUnwrap(content.serverId), eventId: content.eventId))
        XCTAssertNil(router.pendingEnvelope)
    }

    func testBufferedOldAccountRejectedWhenCurrentKeyBecomesReady() throws {
        try assertReplacementRejectsBufferedTap(context(user: "bob"))
    }

    func testBufferedOldDeploymentRejectedWhenCurrentKeyBecomesReady() throws {
        try assertReplacementRejectsBufferedTap(context(deployment: "https://other.test"))
    }

    func testBufferedOldInstallationRejectedWhenCurrentKeyBecomesReady() throws {
        try assertReplacementRejectsBufferedTap(context(installation: "replacement-install"))
    }

    func testBufferedOldLoginRejectedWhenCurrentKeyBecomesReady() throws {
        try assertReplacementRejectsBufferedTap(context(proof: "new-login-proof"))
    }

    private func assertReplacementRejectsBufferedTap(_ replacement: MobileAuthenticationContext) throws {
        let old = context()
        let (oldEnvelope, oldKey) = try encrypted(old, kind: "security", createdAt: Int64(Date().timeIntervalSince1970) - 1860)
        let (currentEnvelope, currentKey) = try encrypted(replacement, kind: "security")
        let delegate = AppDelegate()
        delegate.bufferNotification(userInfo: ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(oldEnvelope))])
        let router = PushNotificationRouter()
        delegate.pushRouter = router
        XCTAssertNil(router.consumeTarget(context: replacement, key: nil))
        XCTAssertEqual(router.pendingEnvelope?.ciphertext, oldEnvelope.ciphertext)
        XCTAssertNotEqual(currentKey.scope, oldKey.scope)
        XCTAssertNil(router.consumeTarget(context: replacement, key: oldKey), "the old key must fail the current login scope")
        XCTAssertNil(router.pendingEnvelope)
        router.enqueue(envelope: oldEnvelope)
        XCTAssertNil(router.consumeTarget(context: replacement, key: nil))
        XCTAssertEqual(router.pendingEnvelope?.ciphertext, oldEnvelope.ciphertext)
        XCTAssertNil(router.consumeTarget(context: replacement, key: currentKey), "reject the old envelope once the CURRENT key is ready")
        XCTAssertNil(router.pendingEnvelope)
        router.enqueue(envelope: currentEnvelope)
        let content = try PushEnvelopeDecoder.decrypt(currentEnvelope, key: currentKey)
        XCTAssertEqual(router.consumeTarget(context: replacement, key: currentKey),
                       .securityDetail(serverId: try XCTUnwrap(content.serverId), eventId: content.eventId))
        XCTAssertNil(router.pendingEnvelope)
    }

    func testTestTapOpensCurrentAccount() {
        var tab = 0
        var servers: [ServerNavigationTarget] = [.detailById("old")]
        var alerts: [ServerDeepLink] = [.alertDetail(alertKey: "old")]
        ContentView.applyDeepLink(.account, selectedTab: &tab, serversPath: &servers, alertsPath: &alerts)
        XCTAssertEqual(tab, 3)
        XCTAssertTrue(servers.isEmpty)
        XCTAssertTrue(alerts.isEmpty)
    }

    func testSecurityColdAndWarmTap() throws {
        let current = context()
        let (envelope, key) = try encrypted(current, kind: "security")
        let content = try PushEnvelopeDecoder.decrypt(envelope, key: key)
        let expected = ServerDeepLink.securityDetail(serverId: try XCTUnwrap(content.serverId), eventId: content.eventId)
        let delegate = AppDelegate()
        delegate.bufferNotification(userInfo: ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))])
        let router = PushNotificationRouter()
        delegate.pushRouter = router
        XCTAssertNil(router.consumeTarget(context: current, key: nil))
        XCTAssertNotNil(router.pendingEnvelope, "keep cold-launch tap until the key is ready")
        XCTAssertEqual(router.consumeTarget(context: current, key: key), expected)
        router.enqueue(envelope: envelope)
        XCTAssertEqual(router.consumeTarget(context: current, key: key), expected)
        router.enqueue(envelope: envelope)
        XCTAssertNil(router.consumeTarget(context: context(user: "bob"), key: key))
        router.enqueue(envelope: envelope)
        XCTAssertNil(router.consumeTarget(context: context(deployment: "https://other.test"), key: key))
        var tab = 3
        var servers: [ServerNavigationTarget] = [.detailById("unrelated")]
        var alerts: [ServerDeepLink] = [.alertDetail(alertKey: "unrelated")]
        ContentView.applyDeepLink(expected, selectedTab: &tab, serversPath: &servers, alertsPath: &alerts)
        XCTAssertEqual(tab, 0)
        XCTAssertEqual(servers, [.security(serverId: try XCTUnwrap(content.serverId), eventId: content.eventId)])
        XCTAssertTrue(alerts.isEmpty)
    }

    func testAlreadyPresentedSecurityColdTapAfterDeliveryExpiry() throws {
        try assertDelayedSecurityTap(coldLaunch: true)
    }

    func testAlreadyPresentedSecurityWarmTapAfterDeliveryExpiry() throws {
        try assertDelayedSecurityTap(coldLaunch: false)
    }

    private func assertDelayedSecurityTap(coldLaunch: Bool) throws {
        let current = context()
        let createdAt = Int64(Date().timeIntervalSince1970) - 1860
        let (envelope, key) = try encrypted(current, kind: "security", createdAt: createdAt)
        let input = UNMutableNotificationContent()
        input.userInfo = ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))]
        // The extension presented this authenticated notification before expiry.
        let presented = PushNotificationRenderer.render(input, key: key, now: createdAt)
        XCTAssertNotNil(presented.userInfo["serverbee_target"])
        XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(envelope, key: key))
        XCTAssertTrue(PushNotificationRenderer.render(input, key: key).userInfo.isEmpty)
        let content = try PushEnvelopeDecoder.decrypt(envelope, key: key, purpose: .notificationTap)
        let expected = ServerDeepLink.securityDetail(serverId: try XCTUnwrap(content.serverId), eventId: content.eventId)
        let delegate = AppDelegate()
        let router = PushNotificationRouter()
        if !coldLaunch { delegate.pushRouter = router }
        delegate.bufferNotification(userInfo: presented.userInfo)
        if coldLaunch { delegate.pushRouter = router }
        XCTAssertNil(router.consumeTarget(context: current, key: nil))
        XCTAssertEqual(router.pendingEnvelope?.ciphertext, envelope.ciphertext)
        XCTAssertEqual(router.consumeTarget(context: current, key: key), expected)
        XCTAssertNil(router.pendingEnvelope)
        var tab = 3
        var servers: [ServerNavigationTarget] = []
        var alerts: [ServerDeepLink] = []
        ContentView.applyDeepLink(expected, selectedTab: &tab, serversPath: &servers, alertsPath: &alerts)
        XCTAssertEqual(tab, 0)
        XCTAssertEqual(servers, [.security(serverId: try XCTUnwrap(content.serverId), eventId: content.eventId)])
    }

    func testStitchedServerRelayCiphertextColdTapValidatesCurrentAccount() throws {
        guard let directory = ProcessInfo.processInfo.environment["SERVERBEE_PUSH_TRACE_DIR"], !directory.isEmpty else {
            throw XCTSkip("Run real Server/Relay trace and pass TEST_RUNNER_SERVERBEE_PUSH_TRACE_DIR")
        }
        let trace = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: directory).appendingPathComponent("trace.json"))) as? [String: Any])
        let registration = try XCTUnwrap(trace["registration"] as? [String: Any])
        let payload = try XCTUnwrap(trace["payload"] as? [String: Any])
        let current = MobileAuthenticationContext(serverUrl: try XCTUnwrap(registration["deployment_id"] as? String),
                                                  userId: try XCTUnwrap(trace["user_id"] as? String), installationId: try XCTUnwrap(trace["installation_id"] as? String),
                                                  generation: UUID(), accessToken: "fixture-access", revocationToken: "fixture-proof", refreshToken: "fixture-refresh")
        let key = PushContentKey(keyId: try XCTUnwrap(registration["content_key_id"] as? String), key: try XCTUnwrap(registration["content_key"] as? String),
                                 deploymentId: current.serverUrl, userId: current.userId, installationId: current.installationId, scope: current.pushScope)
        let delegate = AppDelegate()
        delegate.bufferNotification(userInfo: payload)
        let router = PushNotificationRouter()
        delegate.pushRouter = router
        XCTAssertEqual(router.consumeTarget(context: current, key: key), .account)
    }
}

extension EncryptedPushNavigationTests {
    func testLateTestCategoryTapUsesSameAuthenticatedNavigationPolicy() throws {
        let current = context()
        let (envelope, key) = try encrypted(current, age: 3600)
        XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(envelope, key: key))
        let router = PushNotificationRouter()
        router.enqueue(envelope: envelope)
        XCTAssertEqual(router.consumeTarget(context: current, key: key), .account)
        router.enqueue(envelope: envelope)
        XCTAssertNil(router.consumeTarget(context: context(deployment: "https://other.test"), key: key))
    }
}

extension EncryptedPushNavigationTests {
    func testCombinedCategoriesKeepExclusiveTargetsForDeliveryAndLateNavigation() throws {
        let current = context()
        let created = Int64(Date().timeIntervalSince1970) - 3600
        let eventId = UUID().uuidString.lowercased()
        let serverId = UUID().uuidString.lowercased()
        let taskId = UUID().uuidString.lowercased()
        let encoded = try JSONEncoder().encode(["rule", "server", "", "cycle"]).base64EncodedString()
            .replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
        let alert = AlertPushTarget(alertKey: "v1.\(encoded)", status: "firing", ruleName: "CPU", serverName: "Server")
        let key = PushContentKey(keyId: UUID().uuidString.lowercased(), key: Data(repeating: 7, count: 32).base64EncodedString(),
                                 deploymentId: current.serverUrl, userId: current.userId, installationId: current.installationId, scope: current.pushScope)
        for kind in ["test", "alert", "security", "task_failure", "task_success"] {
            var content = PushContent(kind: kind, deploymentId: current.serverUrl, userId: current.userId, installationId: current.installationId,
                                      eventId: eventId, createdAt: created, expiresAt: created + 1800)
            let expected: ServerDeepLink
            switch kind {
            case "alert":
                content.alert = alert
                expected = .alertDetail(alertKey: alert.alertKey)
            case "security":
                content.serverId = serverId
                content.securityEventId = eventId
                content.securityEventType = "port_scan"
                expected = .securityDetail(serverId: serverId, eventId: eventId)
            case "task_failure", "task_success":
                content.taskRun = TaskRunPushSummary(taskId: taskId, runId: eventId, total: 1, failed: kind == "task_failure" ? 1 : 0,
                                                     timedOut: 0, offline: 0, denied: 0)
                expected = .taskRun(taskId: taskId, runId: eventId)
            default: expected = .account
            }
            let envelope = try sealCombinedContent(content, key: key)
            XCTAssertEqual(try PushEnvelopeDecoder.decrypt(envelope, key: key, now: created), content)
            XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(envelope, key: key))
            let input = UNMutableNotificationContent()
            input.userInfo = ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))]
            XCTAssertNotNil(PushNotificationRenderer.render(input, key: key, now: created).userInfo["serverbee_target"])
            XCTAssertTrue(PushNotificationRenderer.render(input, key: key).userInfo.isEmpty)
            let router = PushNotificationRouter()
            router.enqueue(envelope: envelope)
            XCTAssertEqual(router.consumeTarget(context: current, key: key), expected)
            var mixed: [PushContent] = []
            if kind != "alert" {
                var candidate = content
                candidate.alert = alert
                mixed.append(candidate)
            }
            if !kind.hasPrefix("task_") {
                var candidate = content
                candidate.taskRun = TaskRunPushSummary(taskId: taskId, runId: eventId, total: 1, failed: 1, timedOut: 0, offline: 0, denied: 0)
                mixed.append(candidate)
            }
            if kind != "security" {
                var candidate = content
                candidate.serverId = serverId
                candidate.securityEventId = eventId
                candidate.securityEventType = "port_scan"
                mixed.append(candidate)
            }
            for candidate in mixed {
                let invalid = try sealCombinedContent(candidate, key: key)
                XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(invalid, key: key, now: created))
                XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(invalid, key: key, purpose: .notificationTap))
                router.enqueue(envelope: invalid)
                XCTAssertNil(router.consumeTarget(context: current, key: key))
                input.userInfo = ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(invalid))]
                XCTAssertTrue(PushNotificationRenderer.render(input, key: key, now: created).userInfo.isEmpty, "mixed targets fail before delivery expiry")
                let service = NotificationService()
                service.loadKey = { key }
                var completions = 0
                service.didReceive(UNNotificationRequest(identifier: kind, content: input, trigger: nil)) { result in
                    completions += 1
                    XCTAssertTrue(result.userInfo.isEmpty)
                }
                service.serviceExtensionTimeWillExpire()
                XCTAssertEqual(completions, 1)
            }
        }
    }

    private func sealCombinedContent(_ content: PushContent, key: PushContentKey) throws -> PushEnvelope {
        let identity = try content.identity
        let secret = SymmetricKey(data: try XCTUnwrap(Data(base64Encoded: key.key)))
        let sealed = try AES.GCM.seal(JSONEncoder().encode(content), using: secret,
                                      authenticating: Data("ServerBee.Push.v1|\(key.keyId)|\(identity)".utf8))
        return PushEnvelope(version: 1, keyId: key.keyId, identity: identity, nonce: sealed.nonce.withUnsafeBytes { Data($0) }.base64EncodedString(),
                            ciphertext: (sealed.ciphertext + sealed.tag).base64EncodedString())
    }
}
