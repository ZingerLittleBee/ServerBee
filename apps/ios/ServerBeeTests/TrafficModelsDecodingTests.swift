import XCTest
@testable import ServerBee

/// Decoding + computed-property coverage for the M2 traffic / cost / uptime
/// models, exercised against payloads shaped like the real server responses.
final class TrafficModelsDecodingTests: XCTestCase {
    private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
        try JSONDecoder.snakeCase.decode(T.self, from: Data(json.utf8))
    }

    // MARK: - Traffic

    func test_traffic_withLimitAndPrediction_decodes() throws {
        let json = """
        {
          "cycle_start": "2026-01-01",
          "cycle_end": "2026-12-31",
          "bytes_in": 556461553,
          "bytes_out": 526988216,
          "bytes_total": 1083449769,
          "traffic_limit": 2147483648000,
          "traffic_limit_type": "sum",
          "usage_percent": 0.0504,
          "prediction": { "estimated_total": 2404729975, "estimated_percent": 0.111, "will_exceed": false },
          "daily": [ { "date": "2026-05-26", "bytes_in": 437086764, "bytes_out": 406446667 } ],
          "hourly": []
        }
        """
        let t = try decode(TrafficResponse.self, json)
        XCTAssertEqual(t.bytesTotal, 1_083_449_769)
        XCTAssertEqual(t.trafficLimit, 2_147_483_648_000)
        XCTAssertEqual(t.limitTypeLabel, "Total")
        // "sum" => counted bytes is the total.
        XCTAssertEqual(t.countedBytes, t.bytesTotal)
        XCTAssertEqual(t.prediction?.willExceed, false)
        XCTAssertEqual(t.daily.count, 1)
        XCTAssertEqual(t.daily.first?.bytesTotal, 437_086_764 + 406_446_667)
    }

    func test_traffic_withoutLimit_decodesAndCountsTotal() throws {
        let json = """
        {
          "cycle_start": "2026-06-01",
          "cycle_end": "2026-06-30",
          "bytes_in": 100,
          "bytes_out": 50,
          "bytes_total": 150,
          "traffic_limit": null,
          "traffic_limit_type": null,
          "usage_percent": null,
          "prediction": null,
          "daily": [],
          "hourly": []
        }
        """
        let t = try decode(TrafficResponse.self, json)
        XCTAssertNil(t.trafficLimit)
        XCTAssertNil(t.prediction)
        XCTAssertNil(t.limitTypeLabel)
        XCTAssertEqual(t.countedBytes, 150)
    }

    func test_traffic_uploadLimitType_countsOutbound() throws {
        let json = """
        { "cycle_start": "a", "cycle_end": "b", "bytes_in": 100, "bytes_out": 70,
          "bytes_total": 170, "traffic_limit": 1000, "traffic_limit_type": "up",
          "usage_percent": 7.0, "prediction": null, "daily": [], "hourly": [] }
        """
        let t = try decode(TrafficResponse.self, json)
        XCTAssertEqual(t.limitTypeLabel, "Upload")
        XCTAssertEqual(t.countedBytes, 70)
    }

    // MARK: - Cost

    func test_cost_configured_decodesWithAdvisoriesAndResource() throws {
        let json = """
        {
          "server_id": "abc",
          "configured": true,
          "invalid_reason": null,
          "price": 46.0,
          "currency": "USD",
          "billing_cycle": "yearly",
          "cycle_start": "2026-01-01",
          "cycle_end": "2026-12-31",
          "cycle_days": 365,
          "days_elapsed": 165,
          "days_remaining": 200,
          "cost_per_second": 0.0000014,
          "cost_per_hour": 0.00525,
          "cost_per_day": 0.126,
          "cost_per_month_equivalent": 3.83,
          "cycle_cost_elapsed": 20.79,
          "cycle_cost_remaining": 25.21,
          "cycle_burn_percent": 45.2,
          "resource_value": {
            "cost_per_cpu_core": 1.91, "cost_per_gb_memory": 2.04,
            "cost_per_gb_disk": 0.024, "cost_per_tb_traffic_limit": 1.96,
            "traffic_limit_type": "sum"
          },
          "advisories": ["expired_billing", "low_uptime"]
        }
        """
        let c = try decode(ServerCostInsights.self, json)
        XCTAssertTrue(c.configured)
        XCTAssertNil(c.invalidReason)
        XCTAssertEqual(c.currencyCode, "USD")
        XCTAssertEqual(c.advisories, [.expiredBilling, .lowUptime])
        XCTAssertEqual(c.resourceValue?.costPerCpuCore, 1.91)
    }

    func test_cost_unconfigured_decodesInvalidReason() throws {
        let json = """
        { "server_id": "x", "configured": false, "invalid_reason": "missing_price",
          "price": null, "currency": null, "billing_cycle": null }
        """
        let c = try decode(ServerCostInsights.self, json)
        XCTAssertFalse(c.configured)
        XCTAssertEqual(c.invalidReason, .missingPrice)
        XCTAssertEqual(c.currencyCode, "USD")  // falls back when null
    }

    func test_cost_unknownAdvisory_degradesToUnknown() throws {
        let json = """
        { "server_id": "x", "configured": true,
          "advisories": ["brand_new_advisory_from_future"] }
        """
        let c = try decode(ServerCostInsights.self, json)
        XCTAssertEqual(c.advisories, [.unknown])
    }

    // MARK: - Uptime

    func test_uptime_statusBuckets() throws {
        let json = """
        [
          { "date": "2026-06-01", "total_minutes": 1440, "online_minutes": 1440, "downtime_incidents": 0 },
          { "date": "2026-06-02", "total_minutes": 1440, "online_minutes": 1430, "downtime_incidents": 1 },
          { "date": "2026-06-03", "total_minutes": 1440, "online_minutes": 0,    "downtime_incidents": 0 },
          { "date": "2026-06-04", "total_minutes": 0,    "online_minutes": 0,    "downtime_incidents": 0 }
        ]
        """
        let days = try decode([UptimeDailyEntry].self, json)
        XCTAssertEqual(days[0].status, .operational)
        XCTAssertEqual(days[1].status, .degraded)
        XCTAssertEqual(days[2].status, .down)
        XCTAssertEqual(days[3].status, .noData)
        XCTAssertNil(days[3].ratio)
        XCTAssertEqual(days.totalIncidents, 1)
        XCTAssertEqual(days.daysWithData, 3)
        // overall = (1440+1430+0) / (1440*3)
        let expected = Double(1440 + 1430) / Double(1440 * 3)
        XCTAssertEqual(days.overallRatio ?? -1, expected, accuracy: 0.0001)
    }
}
