import XCTest
@testable import ServerBee

/// Decode/encode coverage for the cross-server overview models (traffic /
/// network-probe fleet roll-ups) and the incident-create server-scope wire
/// contract. Guards the snake_case shapes the project coders rely on.
final class FleetOverviewModelsTests: XCTestCase {

    // MARK: - Traffic overview

    func test_decode_serverTrafficOverview_withLimit() throws {
        let json = """
        {
          "server_id": "s1",
          "name": "web-1",
          "cycle_in": 1000,
          "cycle_out": 500,
          "traffic_limit": 10000,
          "billing_cycle": "monthly",
          "percent_used": 15.0,
          "days_remaining": 12
        }
        """
        let o = try JSONDecoder.snakeCase.decode(ServerTrafficOverview.self, from: Data(json.utf8))
        XCTAssertEqual(o.name, "web-1")
        XCTAssertEqual(o.cycleTotal, 1500)
        XCTAssertEqual(o.trafficLimit, 10000)
        XCTAssertEqual(o.percentUsed, 15.0)
        XCTAssertEqual(o.daysRemaining, 12)
    }

    func test_decode_serverTrafficOverview_noLimit() throws {
        let json = """
        {
          "server_id": "s2",
          "name": "db-1",
          "cycle_in": 42,
          "cycle_out": 8,
          "traffic_limit": null,
          "billing_cycle": null,
          "percent_used": null,
          "days_remaining": 0
        }
        """
        let o = try JSONDecoder.snakeCase.decode(ServerTrafficOverview.self, from: Data(json.utf8))
        XCTAssertNil(o.trafficLimit)
        XCTAssertNil(o.percentUsed)
        XCTAssertEqual(o.cycleTotal, 50)
    }

    // MARK: - Network probe fleet overview

    func test_decode_networkProbeFleetOverview_sparklineWithGaps() throws {
        let json = """
        {
          "server_id": "s1",
          "server_name": "edge-1",
          "online": true,
          "last_probe_at": "2026-06-16T10:00:00Z",
          "targets": [
            {"target_id": "t1", "target_name": "CT", "provider": "ct",
             "avg_latency": 30.0, "min_latency": 10.0, "max_latency": 80.0,
             "packet_loss": 0.05, "availability": 0.95},
            {"target_id": "t2", "target_name": "CU", "provider": "cu",
             "avg_latency": 120.0, "min_latency": 90.0, "max_latency": 200.0,
             "packet_loss": 0.2, "availability": 0.8}
          ],
          "anomaly_count": 3,
          "latency_sparkline": [30.0, null, 45.0],
          "loss_sparkline": [0.0, 0.1, null]
        }
        """
        let o = try JSONDecoder.snakeCase.decode(NetworkProbeFleetOverview.self, from: Data(json.utf8))
        XCTAssertEqual(o.serverName, "edge-1")
        XCTAssertEqual(o.anomalyCount, 3)
        XCTAssertEqual(o.targets.count, 2)
        XCTAssertEqual(o.latencySparkline.count, 3)
        XCTAssertNil(o.latencySparkline[1])
        // Worst (highest) latency / loss across targets.
        XCTAssertEqual(o.worstLatency, 120.0)
        XCTAssertEqual(o.worstLoss, 0.2, accuracy: 0.0001)
    }

    // MARK: - Incident create scope

    func test_encode_createIncident_withServerScope() throws {
        let request = CreateIncidentRequest(
            title: "Outage", severity: "major", isPublic: true, serverIdsJson: ["s1", "s2"]
        )
        let data = try JSONEncoder.snakeCase.encode(request)
        let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        XCTAssertEqual(obj?["title"] as? String, "Outage")
        XCTAssertEqual(obj?["is_public"] as? Bool, true)
        // server_ids_json is an ARRAY on the wire (not a JSON string).
        XCTAssertEqual(obj?["server_ids_json"] as? [String], ["s1", "s2"])
    }

    func test_encode_createIncident_noScopeOmitsKey() throws {
        let request = CreateIncidentRequest(
            title: "Outage", severity: "minor", isPublic: false, serverIdsJson: nil
        )
        let data = try JSONEncoder.snakeCase.encode(request)
        let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        // A nil optional omits the key entirely (= "all servers").
        XCTAssertNil(obj?["server_ids_json"])
    }
}
