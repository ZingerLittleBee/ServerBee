import XCTest
@testable import ServerBee

final class MobileAlertEventIdTests: XCTestCase {
    private func make(status: AlertStatus, updatedAt: String) -> MobileAlertEvent {
        MobileAlertEvent(
            alertKey: "rule-1:server-1",
            ruleId: "rule-1",
            ruleName: "High CPU",
            serverId: "server-1",
            serverName: "vps-a",
            status: status,
            message: "msg",
            triggerCount: 3,
            firstTriggeredAt: "2026-05-20T10:00:00Z",
            lastNotifiedAt: updatedAt,
            resolvedAt: status == .resolved ? updatedAt : nil,
            updatedAt: updatedAt
        )
    }

    func test_id_differs_betweenFiringAndResolved_sameAlertKey() {
        let firing = make(status: .firing, updatedAt: "2026-05-20T10:05:00Z")
        let resolved = make(status: .resolved, updatedAt: "2026-05-20T10:10:00Z")
        XCTAssertNotEqual(firing.id, resolved.id, "Different status must yield distinct IDs")
    }

    func test_id_stableForSameStatusAndTimestamp() {
        let a = make(status: .firing, updatedAt: "2026-05-20T10:05:00Z")
        let b = make(status: .firing, updatedAt: "2026-05-20T10:05:00Z")
        XCTAssertEqual(a.id, b.id)
    }
}
