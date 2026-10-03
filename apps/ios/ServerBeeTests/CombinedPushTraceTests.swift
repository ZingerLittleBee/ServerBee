import Foundation
import UserNotifications
import XCTest
@testable import ServerBee

/// Substitute only the system-owned delivered notification construction. Decode
/// an actual UNNotification so the production foreground delegate is exercised.
@objc(ServerBeeTestsDeliveredNotificationArchive)
private final class DeliveredNotificationArchive: NSObject, NSSecureCoding {
    static var supportsSecureCoding: Bool { true }
    let request: UNNotificationRequest
    init(request: UNNotificationRequest) { self.request = request }
    required init?(coder: NSCoder) { return nil }
    func encode(with coder: NSCoder) {
        coder.encode(Date() as NSDate, forKey: "date")
        coder.encode(request, forKey: "request")
    }
}

@MainActor
final class CombinedPushTraceTests: XCTestCase {
    private func delivered(_ request: UNNotificationRequest) throws -> UNNotification {
        let archive = NSKeyedArchiver(requiringSecureCoding: true)
        archive.setClassName(NSStringFromClass(UNNotification.self), for: DeliveredNotificationArchive.self)
        archive.encode(DeliveredNotificationArchive(request: request), forKey: NSKeyedArchiveRootObjectKey)
        archive.finishEncoding()
        let notification = try XCTUnwrap(NSKeyedUnarchiver.unarchivedObject(ofClass: UNNotification.self, from: archive.encodedData))
        XCTAssertEqual(notification.request.identifier, request.identifier)
        return notification
    }

    func testActualForegroundDelegateAndMatchingSecurityWebSocketCompletePresentationOnce() async throws {
        let input = UNMutableNotificationContent()
        let eventId = UUID().uuidString.lowercased()
        let serverId = UUID().uuidString.lowercased()
        let request = UNNotificationRequest(identifier: eventId, content: input, trigger: nil)
        let notification = try delivered(request)
        let delegate = AppDelegate()
        let push = PushNotificationRouter()
        delegate.pushRouter = push
        let feed = SecurityFeedStore()
        let broadcast = SecurityEventBroadcast(serverId: serverId, eventId: eventId,
            event: SecurityEventPayload(eventType: "port_scan", severity: "high", sourceIp: "203.0.113.17",
                                        startedAt: 100, endedAt: 101, firstSeen: false, detectorSource: "journal"))
        let ws = WebSocketRouter(servers: { _ in }, alerts: { _ in }, security: { feed.ingest($0) })
        let center = UNUserNotificationCenter.current()
        let before = await pendingNotificationIdentifiers(center)
        var completions = 0
        delegate.userNotificationCenter(center, willPresent: notification) { options in
            completions += 1
            XCTAssertEqual(options, [.banner, .badge, .sound])
        }
        ws.dispatch(.securityEvent(broadcast))
        ws.dispatch(.securityEvent(broadcast))
        let after = await pendingNotificationIdentifiers(center)
        XCTAssertEqual(completions, 1)
        XCTAssertEqual(feed.events.map(\.id), [eventId])
        XCTAssertEqual(after, before, "matching WS updates must not schedule another system notification")
        XCTAssertNil(push.pendingEnvelope, "foreground presentation is not a tap")
        XCTAssertNil(push.pendingDeepLink)
    }

    func testStitchedAllServerCategoriesThroughActualExtensionAndAuthenticatedRouter() throws {
        guard let directory = ProcessInfo.processInfo.environment["SERVERBEE_PUSH_TRACE_DIR"], !directory.isEmpty else {
            throw XCTSkip("Run both real Server/Relay trace commands and pass TEST_RUNNER_SERVERBEE_PUSH_TRACE_DIR")
        }
        let data = try Data(contentsOf: URL(fileURLWithPath: directory).appendingPathComponent("trace-categories.json"))
        let trace = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let entries = try XCTUnwrap(trace["entries"] as? [[String: Any]])
        XCTAssertEqual(entries.count, 7)
        var kinds: Set<String> = []
        var alertStatuses: Set<String> = []
        for entry in entries {
            let registration = try XCTUnwrap(entry["registration"] as? [String: Any])
            let payload = try XCTUnwrap(entry["payload"] as? [String: Any])
            let expectedObject = try XCTUnwrap(entry["content"])
            let expected = try JSONDecoder().decode(PushContent.self, from: JSONSerialization.data(withJSONObject: expectedObject))
            let current = MobileAuthenticationContext(serverUrl: expected.deploymentId, userId: expected.userId, installationId: expected.installationId,
                generation: UUID(), accessToken: "fixture-access", revocationToken: "fixture-proof", refreshToken: "fixture-refresh")
            let key = PushContentKey(keyId: try XCTUnwrap(registration["content_key_id"] as? String),
                key: try XCTUnwrap(registration["content_key"] as? String), deploymentId: current.serverUrl,
                userId: current.userId, installationId: current.installationId, scope: current.pushScope)
            let envelope = try JSONDecoder().decode(PushEnvelope.self,
                from: JSONSerialization.data(withJSONObject: try XCTUnwrap(payload["serverbee_envelope"])))
            XCTAssertEqual(try PushEnvelopeDecoder.decrypt(envelope, key: key), expected)
            kinds.insert(expected.kind)
            if let alert = expected.alert { alertStatuses.insert(alert.status) }
            let target = try target(for: expected)
            let input = UNMutableNotificationContent()
            input.userInfo = payload
            let service = NotificationService()
            service.loadKey = { key }
            var completions = 0
            service.didReceive(UNNotificationRequest(identifier: expected.eventId, content: input, trigger: nil)) { rendered in
                completions += 1
                XCTAssertEqual(rendered.userInfo["serverbee_key_id"] as? String, key.keyId)
                XCTAssertEqual((rendered.userInfo["serverbee_target"] as? [String: Any])?["event_id"] as? String, expected.eventId)
                XCTAssertNotEqual(rendered.body, String(localized: "Open ServerBee to view this notification."))
                let delegate = AppDelegate()
                delegate.bufferNotification(userInfo: rendered.userInfo)
                let router = PushNotificationRouter()
                delegate.pushRouter = router
                XCTAssertEqual(router.consumeTarget(context: current, key: key), target)
                router.enqueue(envelope: envelope)
                let replacement = MobileAuthenticationContext(serverUrl: current.serverUrl, userId: "replacement-user",
                    installationId: current.installationId, generation: UUID(), accessToken: "replacement", revocationToken: "replacement-proof", refreshToken: nil)
                XCTAssertNil(router.consumeTarget(context: replacement, key: key))
            }
            service.serviceExtensionTimeWillExpire()
            XCTAssertEqual(completions, 1)
        }
        XCTAssertEqual(kinds, ["test", "alert", "security", "task_failure", "task_success"])
        XCTAssertEqual(alertStatuses, ["firing", "resolved"])
    }

    private func pendingNotificationIdentifiers(_ center: UNUserNotificationCenter) async -> [String] {
        // Keep non-Sendable notification requests inside the system callback.
        await withCheckedContinuation { continuation in
            center.getPendingNotificationRequests { requests in
                continuation.resume(returning: requests.map(\.identifier).sorted())
            }
        }
    }

    private func target(for content: PushContent) throws -> ServerDeepLink {
        switch content.kind {
        case "alert": return .alertDetail(alertKey: try XCTUnwrap(content.alert?.alertKey))
        case "security": return .securityDetail(serverId: try XCTUnwrap(content.serverId), eventId: try XCTUnwrap(content.securityEventId))
        case "task_failure", "task_success":
            return .taskRun(taskId: try XCTUnwrap(content.taskRun?.taskId), runId: try XCTUnwrap(content.taskRun?.runId))
        default: return .account
        }
    }
}
