import XCTest
@testable import ServerBee

/// Decoding + aggregation coverage for the M5 Insights hub: fleet summary, cost
/// overview, service monitors (incl. the flattened detail), incidents, and
/// maintenance. Shapes verified against the live demo backend.
final class InsightsModelsDecodingTests: XCTestCase {
    private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
        try JSONDecoder.snakeCase.decode(T.self, from: Data(json.utf8))
    }

    // MARK: - Fleet summary aggregation

    func test_fleetSummary_aggregates() {
        let servers = [
            makeServer(id: "a", online: true, cpu: 10, mem: 20, netIn: 100, netOut: 50, inT: 1000, outT: 500),
            makeServer(id: "b", online: true, cpu: 30, mem: 40, netIn: 200, netOut: 100, inT: 2000, outT: 1000),
            makeServer(id: "c", online: false, cpu: 99, mem: 99, netIn: 999, netOut: 999, inT: 9999, outT: 9999)
        ]
        let fleet = FleetSummary.from(servers)
        XCTAssertEqual(fleet.total, 3)
        XCTAssertEqual(fleet.online, 2)
        XCTAssertEqual(fleet.offline, 1)
        XCTAssertEqual(fleet.avgCpu ?? 0, 20, accuracy: 0.001) // offline excluded
        XCTAssertEqual(fleet.avgMemory ?? 0, 30, accuracy: 0.001)
        XCTAssertEqual(fleet.totalNetworkIn, 300) // offline excluded
        XCTAssertEqual(fleet.totalNetworkOut, 150)
        XCTAssertEqual(fleet.totalInTransfer, 3000)
        XCTAssertEqual(fleet.totalOutTransfer, 1500)
    }

    func test_fleetSummary_empty() {
        let fleet = FleetSummary.from([])
        XCTAssertEqual(fleet.total, 0)
        XCTAssertNil(fleet.avgCpu)
        XCTAssertEqual(fleet.totalNetworkIn, 0)
    }

    // MARK: - Cost overview

    func test_costOverview_decodes() throws {
        let json = """
        { "currencies": [
            { "currency": "USD", "configured_server_count": 2, "monthly_equivalent_total": 28.83,
              "daily_total": 0.95, "cycle_elapsed_total": 12.4 } ],
          "servers": [
            { "server_id": "s1", "name": "BWG", "configured": true, "invalid_reason": null,
              "currency": "USD", "billing_cycle": "monthly", "cost_per_day": 0.5,
              "cost_per_month_equivalent": 15.0, "cycle_burn_percent": 40.0, "days_remaining": 18,
              "value_score": null } ] }
        """
        let resp = try decode(CostOverviewResponse.self, json)
        XCTAssertEqual(resp.currencies[0].currency, "USD")
        XCTAssertEqual(resp.currencies[0].monthlyEquivalentTotal, 28.83, accuracy: 0.001)
        XCTAssertEqual(resp.currencies[0].configuredServerCount, 2)
        XCTAssertEqual(resp.servers[0].name, "BWG")
        XCTAssertEqual(resp.servers[0].daysRemaining, 18)
    }

    // MARK: - Service monitor (flattened detail)

    func test_monitorWithRecord_flattenDecodes() throws {
        let json = """
        { "id": "m1", "name": "111", "monitor_type": "whois", "target": "google.com",
          "interval": 300, "config_json": "{}", "notification_group_id": null, "retry_count": 1,
          "server_ids_json": null, "enabled": true, "last_status": true, "consecutive_failures": 0,
          "last_checked_at": "2026-06-14T17:07:31.988260419Z",
          "created_at": "2026-04-13T16:46:31.102988671Z", "updated_at": "2026-06-14T17:07:31.988261196Z",
          "latest_record": { "id": 17479, "monitor_id": "m1", "success": true, "latency": 8.36,
            "detail_json": "{\\"days_remaining\\":822}", "error": null, "time": "2026-06-14T17:07:31.986501099Z" } }
        """
        let detail = try decode(MonitorWithRecord.self, json)
        XCTAssertEqual(detail.monitor.name, "111")
        XCTAssertEqual(detail.monitor.typeLabel, "WHOIS")
        XCTAssertEqual(detail.monitor.isUp, true)
        XCTAssertEqual(detail.latestRecord?.id, 17479)
        XCTAssertEqual(detail.latestRecord?.latency ?? 0, 8.36, accuracy: 0.01)
    }

    func test_serviceMonitor_listDecodes() throws {
        let json = """
        [{ "id": "m1", "name": "ssl-check", "monitor_type": "ssl", "target": "example.com:443",
           "interval": 600, "config_json": "{}", "notification_group_id": "g1", "retry_count": 2,
           "server_ids_json": "[\\"s1\\"]", "enabled": false, "last_status": null,
           "consecutive_failures": 0, "last_checked_at": null,
           "created_at": "2026-04-13T16:46:31Z", "updated_at": "2026-04-13T16:46:31Z" }]
        """
        let monitors = try decode([ServiceMonitor].self, json)
        XCTAssertEqual(monitors[0].typeLabel, "SSL")
        XCTAssertNil(monitors[0].isUp)
        XCTAssertFalse(monitors[0].enabled)
    }

    // MARK: - Incidents / maintenance

    func test_incident_decodes() throws {
        let json = """
        [{ "id": "i1", "title": "API latency", "status": "investigating", "severity": "major",
           "server_ids_json": null, "is_public": true,
           "created_at": "2026-06-14T10:00:00Z", "updated_at": "2026-06-14T10:00:00Z", "resolved_at": null }]
        """
        let incidents = try decode([Incident].self, json)
        XCTAssertEqual(incidents[0].statusLabel, "Investigating")
        XCTAssertFalse(incidents[0].isResolved)
        XCTAssertTrue(incidents[0].isPublic)
    }

    func test_maintenance_decodes() throws {
        let json = """
        [{ "id": "mw1", "title": "DB upgrade", "description": "Postgres 16",
           "start_at": "2026-06-20T02:00:00Z", "end_at": "2026-06-20T04:00:00Z",
           "server_ids_json": null, "is_public": true, "active": true,
           "created_at": "2026-06-14T10:00:00Z", "updated_at": "2026-06-14T10:00:00Z" }]
        """
        let windows = try decode([Maintenance].self, json)
        XCTAssertEqual(windows[0].title, "DB upgrade")
        XCTAssertTrue(windows[0].active)
    }

    func test_createIncidentUpdateRequest_encodes() throws {
        let data = try JSONEncoder.snakeCase.encode(CreateIncidentUpdateRequest(status: "resolved", message: "Fixed"))
        let json = String(data: data, encoding: .utf8)!
        XCTAssertTrue(json.contains("\"status\":\"resolved\""))
        XCTAssertTrue(json.contains("\"message\":\"Fixed\""))
    }

    func test_createIncidentRequest_omitsNil() throws {
        let data = try JSONEncoder.snakeCase.encode(CreateIncidentRequest(title: "X", severity: nil, isPublic: nil))
        let json = String(data: data, encoding: .utf8)!
        XCTAssertTrue(json.contains("\"title\":\"X\""))
        XCTAssertFalse(json.contains("severity"))
        XCTAssertFalse(json.contains("is_public"))
    }

    // MARK: - Helpers

    private func makeServer(id: String, online: Bool, cpu: Double, mem: Double,
                            netIn: Int64, netOut: Int64, inT: Int64, outT: Int64) -> ServerStatus {
        let json = """
        { "id": "\(id)", "name": "\(id)", "online": \(online), "cpu_usage": \(cpu),
          "memory_used": \(Int(mem)), "memory_total": 100,
          "network_in": \(netIn), "network_out": \(netOut),
          "net_in_transfer": \(inT), "net_out_transfer": \(outT) }
        """
        // memory_percent derives from used/total → mem%.
        return try! JSONDecoder.snakeCase.decode(ServerStatus.self, from: Data(json.utf8))
    }
}
