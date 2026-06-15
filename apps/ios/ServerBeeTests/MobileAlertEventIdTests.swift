import XCTest
@testable import ServerBee

final class MobileAlertEventIdTests: XCTestCase {
    private func make(status: AlertStatus, eventAt: String) -> MobileAlertEvent {
        MobileAlertEvent(
            ruleId: "rule-1",
            ruleName: "High CPU",
            serverId: "server-1",
            serverName: "vps-a",
            status: status,
            eventAt: eventAt,
            resolvedAt: status == .resolved ? eventAt : nil,
            count: 3
        )
    }

    func test_id_differs_betweenFiringAndResolved_sameAlertKey() {
        let firing = make(status: .firing, eventAt: "2026-05-20T10:05:00Z")
        let resolved = make(status: .resolved, eventAt: "2026-05-20T10:10:00Z")
        XCTAssertNotEqual(firing.id, resolved.id, "Different status must yield distinct IDs")
    }

    func test_id_stableForSameStatusAndTimestamp() {
        let a = make(status: .firing, eventAt: "2026-05-20T10:05:00Z")
        let b = make(status: .firing, eventAt: "2026-05-20T10:05:00Z")
        XCTAssertEqual(a.id, b.id)
    }

    func test_alertKey_isRuleColonServer() {
        XCTAssertEqual(make(status: .firing, eventAt: "2026-05-20T10:05:00Z").alertKey, "rule-1:server-1")
    }

    /// Regression: the list DTO (`AlertEventResponse`) carries only rule/server
    /// identity, status, `event_at`, optional `resolved_at`, and `count` — NOT
    /// the detail DTO's `alert_key`/`message`/`trigger_count`. Decoding the real
    /// list shape must succeed (it previously threw keyNotFound).
    func test_decodesRealListResponseShape() throws {
        let json = """
        [
          { "rule_id": "r1", "rule_name": "High CPU", "server_id": "s1",
            "server_name": "vps-a", "status": "firing",
            "event_at": "2026-05-20T10:05:00Z", "resolved_at": null, "count": 4 },
          { "rule_id": "r2", "rule_name": "Disk full", "server_id": "s2",
            "server_name": "vps-b", "status": "resolved",
            "event_at": "2026-05-20T11:00:00Z",
            "resolved_at": "2026-05-20T11:00:00Z", "count": 1 }
        ]
        """
        let events = try JSONDecoder.snakeCase.decode([MobileAlertEvent].self, from: Data(json.utf8))
        XCTAssertEqual(events.count, 2)
        XCTAssertEqual(events[0].status, .firing)
        XCTAssertEqual(events[0].count, 4)
        XCTAssertEqual(events[0].alertKey, "r1:s1")
        XCTAssertNil(events[0].resolvedAt)
        XCTAssertEqual(events[1].status, .resolved)
        XCTAssertEqual(events[1].resolvedAt, "2026-05-20T11:00:00Z")
    }
}
