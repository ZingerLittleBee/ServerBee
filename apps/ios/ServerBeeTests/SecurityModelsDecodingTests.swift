import XCTest
@testable import ServerBee

/// Decoding + mapping coverage for M4 security models and the WS security_event
/// frame.
final class SecurityModelsDecodingTests: XCTestCase {
    private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
        try JSONDecoder.snakeCase.decode(T.self, from: Data(json.utf8))
    }

    // MARK: - REST DTO + evidence

    func test_eventList_decodesWithCursorAndEvidence() throws {
        let json = """
        { "items": [
            { "id": "e1", "server_id": "s1", "event_type": "ssh_brute_force",
              "severity": "high", "source_ip": "87.251.64.149", "source_port": null,
              "username": null, "started_at": "2026-05-27T09:50:00Z",
              "ended_at": "2026-05-27T09:51:00Z", "first_seen": false,
              "detector_source": "journal",
              "evidence": { "kind": "ssh_brute_force", "failed_count": 10,
                "distinct_users": 1, "invalid_user_count": 0, "sample_users": ["root"],
                "window_seconds": 60, "threshold": 10 },
              "created_at": "2026-05-27T09:51:05Z" }
          ],
          "next_cursor": "YjI0MDM" }
        """
        let list = try decode(SecurityEventList.self, json)
        XCTAssertEqual(list.items.count, 1)
        XCTAssertEqual(list.nextCursor, "YjI0MDM")
        let ev = list.items[0]
        XCTAssertEqual(ev.evidence?.failedCount, 10)
        XCTAssertEqual(ev.evidence?.sampleUsers, ["root"])
        // detail rows skip nil fields but include populated ones
        let labels = ev.evidence?.detailRows.map(\.0) ?? []
        XCTAssertTrue(labels.contains("Failed attempts"))
        XCTAssertTrue(labels.contains("Threshold"))
        XCTAssertEqual(ev.evidence?.summary, "10 failed logins")
    }

    func test_sshLoginEvidence_summaryAndDetail() throws {
        let json = """
        { "kind": "ssh_login", "auth_method": "password" }
        """
        let ev = try decode(SecurityEvidence.self, json)
        XCTAssertEqual(ev.authMethod, "password")
        XCTAssertEqual(ev.summary, "via password")
    }

    func test_unknownEvidenceKind_decodesEmpty() throws {
        let ev = try decode(SecurityEvidence.self, "{ \"kind\": \"future_thing\", \"weird\": 1 }")
        XCTAssertEqual(ev.kind, "future_thing")
        XCTAssertTrue(ev.detailRows.isEmpty)
        XCTAssertNil(ev.summary)
    }

    func test_stats_decodes() throws {
        let buckets = try decode([StatsBucket].self,
            "[{\"key\":\"ssh_brute_force\",\"count\":2446},{\"key\":\"ssh_login\",\"count\":352}]")
        XCTAssertEqual(buckets.first?.count, 2446)
    }

    // MARK: - WS broadcast → display model

    func test_securityEventBroadcast_mapsToDisplayEvent() throws {
        let json = """
        { "server_id": "s1", "event_id": "evt-9",
          "event": { "event_type": "port_scan", "severity": "high",
            "source_ip": "203.0.113.5", "source_port": 1234, "username": null,
            "started_at": 1700000000, "ended_at": 1700000060, "first_seen": true,
            "detector_source": "conntrack",
            "evidence": { "kind": "port_scan", "distinct_ports": 128,
              "sample_ports": [22, 80, 443], "total_attempts": 512,
              "window_seconds": 300, "threshold": 100, "blocked_count": 256 } } }
        """
        let broadcast = try decode(SecurityEventBroadcast.self, json)
        let event = SecurityEvent(broadcast: broadcast)
        XCTAssertEqual(event.id, "evt-9")
        XCTAssertEqual(event.serverId, "s1")
        XCTAssertEqual(event.eventType, "port_scan")
        XCTAssertEqual(event.sourcePort, 1234)
        XCTAssertTrue(event.firstSeen)
        XCTAssertEqual(event.evidence?.distinctPorts, 128)
        XCTAssertEqual(event.evidence?.summary, "128 ports scanned")
        // epoch seconds → Date
        XCTAssertEqual(event.date?.timeIntervalSince1970 ?? -1, 1_700_000_000, accuracy: 1)
    }

    // MARK: - BrowserMessage routing

    func test_browserMessage_securityEvent_decodes() throws {
        let json = """
        { "type": "security_event", "server_id": "s1", "event_id": "e2",
          "event": { "event_type": "ssh_login", "severity": "info",
            "source_ip": "1.2.3.4", "started_at": 1700000000, "ended_at": 1700000000,
            "first_seen": false, "detector_source": "auth_log",
            "evidence": { "kind": "ssh_login", "auth_method": "publickey" } } }
        """
        let msg = try decode(BrowserMessage.self, json)
        guard case .securityEvent(let b) = msg else {
            return XCTFail("expected .securityEvent, got \(msg)")
        }
        XCTAssertEqual(b.eventId, "e2")
        XCTAssertEqual(b.event.severity, "info")
    }

    func test_browserMessage_unknownType_decodesToUnknown() throws {
        let msg = try decode(BrowserMessage.self, "{ \"type\": \"ip_quality_update\", \"foo\": 1 }")
        guard case .unknown = msg else {
            return XCTFail("expected .unknown, got \(msg)")
        }
    }

    // MARK: - Feed store

    @MainActor
    func test_feedStore_ingestDedupesAndFilters() throws {
        let store = SecurityFeedStore()
        let mk: (String, String) -> SecurityEventBroadcast = { id, server in
            SecurityEventBroadcast(
                serverId: server, eventId: id,
                event: SecurityEventPayload(
                    eventType: "ssh_login", severity: "info", sourceIp: "1.1.1.1",
                    sourcePort: nil, username: nil, startedAt: 1, endedAt: 1,
                    firstSeen: false, detectorSource: "journal", evidence: nil
                )
            )
        }
        store.ingest(mk("a", "s1"))
        store.ingest(mk("a", "s1"))   // duplicate id ignored
        store.ingest(mk("b", "s2"))
        XCTAssertEqual(store.events.count, 2)
        XCTAssertEqual(store.events(forServer: "s1").map(\.id), ["a"])
    }
}
