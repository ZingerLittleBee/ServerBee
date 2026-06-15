import XCTest
@testable import ServerBee

/// Decoding + logic coverage for the M3 network-probe and traceroute models.
final class NetworkModelsDecodingTests: XCTestCase {
    private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
        try JSONDecoder.snakeCase.decode(T.self, from: Data(json.utf8))
    }

    // MARK: - Probe summary / records

    func test_serverSummary_decodes() throws {
        let json = """
        { "server_id": "s1", "server_name": "GEN2", "online": true,
          "targets": [
            { "target_id": "t1", "target_name": "BJ", "provider": "ct",
              "avg_latency": 12.5, "min_latency": 10.0, "max_latency": 20.0,
              "packet_loss": 0.0, "availability": 1.0 }
          ],
          "last_probe_at": "2026-06-14T10:00:00Z", "anomaly_count": 3 }
        """
        let s = try decode(NetworkProbeServerSummary.self, json)
        XCTAssertTrue(s.online)
        XCTAssertEqual(s.anomalyCount, 3)
        XCTAssertEqual(s.targets.first?.avgLatency, 12.5)
        XCTAssertEqual(s.targets.first?.id, "t1")
    }

    func test_probeRecord_decodesAndParsesDate() throws {
        let json = """
        { "server_id": "s1", "target_id": "t1", "timestamp": "2026-06-14T10:00:00Z",
          "avg_latency": 15.0, "min_latency": null, "max_latency": null,
          "packet_loss": 0.25, "packet_sent": 4, "packet_received": 3 }
        """
        let r = try decode(ProbeRecordDto.self, json)
        XCTAssertEqual(r.avgLatency, 15.0)
        XCTAssertEqual(r.packetLoss, 0.25)
        XCTAssertNotNil(r.date)
    }

    func test_anomaly_latencyVsLoss() throws {
        let json = """
        [ { "timestamp": "2026-06-14T10:00:00Z", "target_id": "t1", "target_name": "BJ",
            "anomaly_type": "high_latency", "value": 350.0 },
          { "timestamp": "2026-06-14T09:00:00Z", "target_id": "t2", "target_name": "SH",
            "anomaly_type": "packet_loss", "value": 0.4 } ]
        """
        let a = try decode([NetworkProbeAnomaly].self, json)
        XCTAssertTrue(a[0].isLatency)
        XCTAssertFalse(a[1].isLatency)
    }

    // MARK: - Provider mapping

    func test_provider_mapping_handlesCodesAndFullNames() {
        XCTAssertEqual(NetworkProvider.order(for: "ct"), NetworkProvider.order(for: "Telecom"))
        XCTAssertEqual(NetworkProvider.order(for: "cu"), NetworkProvider.order(for: "Unicom"))
        XCTAssertEqual(NetworkProvider.order(for: "cm"), NetworkProvider.order(for: "Mobile"))
        XCTAssertEqual(NetworkProvider.label(for: "Telecom"), NetworkProvider.label(for: "ct"))
        XCTAssertEqual(NetworkProvider.label(for: "international"), "International")
        // unknown providers keep their own capitalised label and sort last
        XCTAssertEqual(NetworkProvider.order(for: "aws"), 4)
        XCTAssertEqual(NetworkProvider.label(for: "aws"), "Aws")
    }

    // MARK: - Traceroute hop schemas

    func test_hop_legacy_averagesRtt() throws {
        let json = """
        { "hop": 3, "ip": "10.0.0.1", "hostname": "gw", "asn": "AS1",
          "rtt1": 9.0, "rtt2": 11.0, "rtt3": 10.0 }
        """
        let h = try decode(TracerouteHop.self, json)
        XCTAssertEqual(h.primaryIP, "10.0.0.1")
        XCTAssertEqual(h.displayLatency ?? -1, 10.0, accuracy: 0.001)
        XCTAssertEqual(h.extraIPCount, 0)
        XCTAssertFalse(h.isUnresponsive)
    }

    func test_hop_trippy_usesAvgAndEcmpAndLossRatio() throws {
        let json = """
        { "hop": 5, "ips": ["1.2.3.4", "1.2.3.5", "1.2.3.6"],
          "total_sent": 10, "total_recv": 9, "loss_pct": 50.0, "avg_ms": 42.0,
          "best_ms": 40.0, "worst_ms": 45.0, "jitter_ms": 1.2 }
        """
        let h = try decode(TracerouteHop.self, json)
        XCTAssertEqual(h.primaryIP, "1.2.3.4")
        XCTAssertEqual(h.extraIPCount, 2)
        XCTAssertEqual(h.displayLatency, 42.0)
        XCTAssertEqual(h.lossRatio ?? -1, 0.5, accuracy: 0.001)
    }

    func test_hop_timeout_isUnresponsive() throws {
        let h = try decode(TracerouteHop.self, "{ \"hop\": 8 }")
        XCTAssertNil(h.primaryIP)
        XCTAssertNil(h.displayLatency)
        XCTAssertTrue(h.isUnresponsive)
    }

    func test_snapshot_withError_decodes() throws {
        let json = """
        { "request_id": "r1", "target": "1.1.1.1", "protocol": "legacy",
          "started_at": 1781447838123, "completed_at": 1781447846445,
          "round": 1, "total_rounds": 1, "completed": true,
          "hops": [], "error": "traceroute not installed" }
        """
        let snap = try decode(TracerouteSnapshot.self, json)
        XCTAssertTrue(snap.completed)
        XCTAssertEqual(snap.error, "traceroute not installed")
        XCTAssertTrue(snap.hops.isEmpty)
        XCTAssertEqual(snap.completedAt, 1781447846445)
    }

    func test_historySummary_decodesAndDerivesDate() throws {
        let json = """
        { "request_id": "r1", "target": "1.1.1.1", "protocol": "icmp",
          "started_at": 1781447838000, "completed_at": null,
          "hop_count": 12, "has_error": false }
        """
        let rec = try decode(TracerouteRecordSummary.self, json)
        XCTAssertEqual(rec.hopCount, 12)
        XCTAssertFalse(rec.hasError)
        XCTAssertEqual(rec.startedDate.timeIntervalSince1970, 1_781_447_838.0, accuracy: 0.001)
    }

    // MARK: - Trigger request encodes "protocol" key

    func test_triggerRequest_encodesProtocolKey() throws {
        let body = TriggerTracerouteRequest(target: "8.8.8.8", protocolValue: .tcp)
        let data = try JSONEncoder.snakeCase.encode(body)
        let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        XCTAssertEqual(obj?["target"] as? String, "8.8.8.8")
        XCTAssertEqual(obj?["protocol"] as? String, "tcp")
    }
}
