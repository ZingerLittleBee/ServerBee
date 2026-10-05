import XCTest
@testable import ServerBee

final class RenewalDateTests: XCTestCase {
    private func object(_ request: UpdateServerRequest) throws -> [String: Any] {
        let data = try JSONEncoder.snakeCase.encode(request)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    func test_serverConfigDecodesSelectedDateAndBillingTimezone() throws {
        let json = """
        {"id":"s1","name":"Server","renewal":{"enabled":false,
        "billing_timezone":"America/Los_Angeles","expiry_date":"2026-03-08",
        "confirmed_expired_at":"2026-03-09T06:59:59Z","deadline_origin":"confirmed",
        "occurrence_id":"occurrence-1"}}
        """
        let config = try JSONDecoder.snakeCase.decode(ServerConfig.self, from: Data(json.utf8))
        XCTAssertEqual(config.renewal?.expiryDate, "2026-03-08")
        XCTAssertEqual(config.renewal?.billingTimezone, "America/Los_Angeles")
        XCTAssertEqual(config.renewal?.confirmedExpiredAt, "2026-03-09T06:59:59Z")
        XCTAssertEqual(config.renewal?.deadlineOrigin, "confirmed")
        XCTAssertEqual(config.renewal?.occurrenceId, "occurrence-1")
        XCTAssertEqual(config.renewal?.enabled, false)
    }

    func test_renewalDateRequestDistinguishesOmitClearAndSet() throws {
        XCTAssertNil(try object(UpdateServerRequest())["renewal"])
        var request = UpdateServerRequest()
        request.renewal = UpdateRenewalRequest(expiryDate: .set("2026-03-08"))
        var renewal = try XCTUnwrap(try object(request)["renewal"] as? [String: Any])
        XCTAssertEqual(renewal["expiry_date"] as? String, "2026-03-08")
        XCTAssertNil(renewal["billing_timezone"])
        XCTAssertNil(try object(request)["expired_at"])
        request.renewal = UpdateRenewalRequest(expiryDate: .clear)
        renewal = try XCTUnwrap(try object(request)["renewal"] as? [String: Any])
        XCTAssertTrue(renewal["expiry_date"] is NSNull)
        request.renewal = UpdateRenewalRequest(billingTimezone: .set("Asia/Tokyo"))
        renewal = try XCTUnwrap(try object(request)["renewal"] as? [String: Any])
        XCTAssertEqual(renewal["billing_timezone"] as? String, "Asia/Tokyo")
        XCTAssertNil(renewal["expiry_date"])
        request.renewal = UpdateRenewalRequest(billingTimezone: .clear)
        renewal = try XCTUnwrap(try object(request)["renewal"] as? [String: Any])
        XCTAssertTrue(renewal["billing_timezone"] is NSNull)
    }

    @MainActor
    func test_unchangedFullFormSavePreservesLegacyInstant() throws {
        let config = try JSONDecoder.snakeCase.decode(ServerConfig.self, from: Data("""
        {"id":"s1","name":"Server","expired_at":"2026-03-08T00:30:00Z",
        "renewal":{"enabled":false,"billing_timezone":"UTC","expiry_date":"2026-03-08",
        "confirmed_expired_at":"2026-03-08T00:30:00Z","deadline_origin":"confirmed","occurrence_id":null}}
        """.utf8))
        let model = EditServerViewModel()
        model.prefill(from: config)
        model.name = "Renamed"
        model.billingCycle = "quarterly"
        model.priceText = "12.50"
        let request = try object(model.buildRequest())
        XCTAssertNil(request["expired_at"])
        XCTAssertNil(request["renewal"])
        XCTAssertEqual(request["billing_cycle"] as? String, "quarterly")
    }

    @MainActor
    func test_timezoneOnlyEditPreservesSelectedDateWithoutConfirmingIt() throws {
        let config = try selectedConfig()
        let model = EditServerViewModel()
        model.prefill(from: config)
        model.billingTimezone = "Asia/Tokyo"
        XCTAssertEqual(BillingDate.string(from: model.expiryDate, timezone: "Asia/Tokyo"), "2026-03-08")
        let request = try object(model.buildRequest())
        let renewal = try XCTUnwrap(request["renewal"] as? [String: Any])
        XCTAssertEqual(renewal["billing_timezone"] as? String, "Asia/Tokyo")
        XCTAssertNil(renewal["expiry_date"])
        XCTAssertNil(request["expired_at"])
    }

    @MainActor
    func test_timezoneEditThroughInvalidIntermediateValuePreservesSelectedDate() throws {
        let model = EditServerViewModel()
        model.prefill(from: try selectedConfig())
        model.billingTimezone = "Asia/"
        model.billingTimezone = "Asia/Tokyo"
        XCTAssertEqual(BillingDate.string(from: model.expiryDate, timezone: "Asia/Tokyo"), "2026-03-08")
        let request = try object(model.buildRequest())
        let renewal = try XCTUnwrap(request["renewal"] as? [String: Any])
        XCTAssertEqual(renewal["billing_timezone"] as? String, "Asia/Tokyo")
        XCTAssertNil(renewal["expiry_date"])
    }

    @MainActor
    func test_changedDateSendsLiteralDateAndClearingSendsNull() throws {
        let model = EditServerViewModel()
        model.prefill(from: try selectedConfig())
        model.expiryDate = try XCTUnwrap(BillingDate.date(from: "2028-02-29", timezone: model.billingTimezone))
        var request = try object(model.buildRequest())
        var renewal = try XCTUnwrap(request["renewal"] as? [String: Any])
        XCTAssertEqual(renewal["expiry_date"] as? String, "2028-02-29")
        XCTAssertNil(renewal["billing_timezone"])
        XCTAssertNil(request["expired_at"])
        model.hasExpiry = false
        request = try object(model.buildRequest())
        renewal = try XCTUnwrap(request["renewal"] as? [String: Any])
        XCTAssertTrue(renewal["expiry_date"] is NSNull)
        XCTAssertNil(request["expired_at"])
    }

    func test_datePickerInstantUsesBillingTimezoneAcrossDST() throws {
        let date = try XCTUnwrap(BillingDate.date(from: "2026-03-08", timezone: "America/Los_Angeles"))
        XCTAssertEqual(date.timeIntervalSince1970, 1_772_996_400)
        XCTAssertEqual(BillingDate.string(from: date, timezone: "America/Los_Angeles"), "2026-03-08")
        XCTAssertEqual(BillingDate.string(from: date, timezone: "Asia/Tokyo"), "2026-03-09")
        XCTAssertNil(BillingDate.date(from: "2026-02-29", timezone: "UTC"))
    }

    func test_datePickerSupportsLocalDateWhoseMidnightDoesNotExist() throws {
        let date = try XCTUnwrap(BillingDate.date(from: "2018-11-04", timezone: "America/Sao_Paulo"))
        XCTAssertEqual(BillingDate.string(from: date, timezone: "America/Sao_Paulo"), "2018-11-04")
        let utc = ISO8601DateFormatter.shared.date(from: "2018-11-04T14:00:00Z")
        XCTAssertEqual(date, utc)
    }

    func test_selectedDateDisplayUsesStoredTimezoneAtUTCDateBoundary() throws {
        let config = try selectedConfig()
        XCTAssertEqual(config.expiryLocalDate, "2026-03-08")
    }

    private func selectedConfig() throws -> ServerConfig {
        try JSONDecoder.snakeCase.decode(ServerConfig.self, from: Data("""
        {"id":"s1","name":"Server","expired_at":"2026-03-09T06:59:59Z",
        "renewal":{"enabled":true,"billing_timezone":"America/Los_Angeles","expiry_date":"2026-03-08",
        "confirmed_expired_at":"2026-02-09T07:59:59Z","deadline_origin":"projected","occurrence_id":"o2"}}
        """.utf8))
    }
}
