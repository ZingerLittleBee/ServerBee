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

    func test_deadlineDisplayUsesServerProvenance() throws {
        for origin in ["confirmed", "projected", "frozen"] {
            let config = try JSONDecoder.snakeCase.decode(ServerConfig.self, from: Data("""
            {"id":"s1","name":"Server","renewal":{"enabled":false,
            "billing_timezone":"America/Los_Angeles","expiry_date":"2030-01-31",
            "confirmed_expired_at":"2029-12-01T07:59:59Z","deadline_origin":"\(origin)"}}
            """.utf8))
            let renewal = try XCTUnwrap(config.renewal)
            XCTAssertNil(renewal.occurrenceId)
            XCTAssertNotNil(renewal.deadlineOriginLabel)
            XCTAssertNotNil(renewal.deadlineExplanation)
            XCTAssertEqual(config.expiryLocalDate, "2030-01-31")
            let confirmed = try XCTUnwrap(renewal.confirmedDisplayDate)
            let expected = try XCTUnwrap(BillingDate.date(from: "2029-11-30", timezone: "America/Los_Angeles"))
            XCTAssertEqual(confirmed, BillingDate.display(from: expected, timezone: "America/Los_Angeles"))
        }
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

    func test_enabledRequestOmitsOrEncodesFalse() throws {
        var request = UpdateServerRequest()
        request.renewal = UpdateRenewalRequest(enabled: true)
        var renewal = try XCTUnwrap(try object(request)["renewal"] as? [String: Any])
        XCTAssertEqual(renewal["enabled"] as? Bool, true)
        XCTAssertNil(renewal["expiry_date"])
        request.renewal = UpdateRenewalRequest(enabled: false, expiryDate: .clear)
        renewal = try XCTUnwrap(try object(request)["renewal"] as? [String: Any])
        XCTAssertEqual(renewal["enabled"] as? Bool, false)
        XCTAssertTrue(renewal["expiry_date"] is NSNull)
        request.renewal = UpdateRenewalRequest(expiryDate: .set("2030-01-31"))
        renewal = try XCTUnwrap(try object(request)["renewal"] as? [String: Any])
        XCTAssertNil(renewal["enabled"])
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
    func test_switchAndUnchangedSavesDoNotConfirm() throws {
        let model = EditServerViewModel()
        model.prefill(from: try selectedConfig())
        XCTAssertTrue(model.automaticRenewal)
        XCTAssertNil(try object(model.buildRequest())["renewal"])
        model.automaticRenewal = false
        let request = try object(model.buildRequest())
        let renewal = try XCTUnwrap(request["renewal"] as? [String: Any])
        XCTAssertEqual(renewal["enabled"] as? Bool, false)
        XCTAssertNil(renewal["expiry_date"])
        XCTAssertNil(renewal["billing_timezone"])
        XCTAssertNil(request["expired_at"])
        model.automaticRenewal = true
        XCTAssertNil(try object(model.buildRequest())["renewal"])
    }

    @MainActor
    func test_enablingSendsOnlyChangedSwitchIntent() throws {
        var config = try selectedConfig()
        config.renewal = ServerRenewal(
            enabled: false, billingTimezone: "America/Los_Angeles", expiryDate: "2026-03-08",
            confirmedExpiredAt: "2026-03-09T06:59:59Z", deadlineOrigin: "confirmed", occurrenceId: "opaque-id"
        )
        let model = EditServerViewModel()
        model.prefill(from: config)
        XCTAssertFalse(model.automaticRenewal)
        XCTAssertNil(model.automaticRenewalPrerequisiteMessage)
        model.automaticRenewal = true
        XCTAssertNil(model.renewalValidationMessage)
        let request = try object(model.buildRequest())
        let renewal = try XCTUnwrap(request["renewal"] as? [String: Any])
        XCTAssertEqual(renewal["enabled"] as? Bool, true)
        XCTAssertNil(renewal["expiry_date"])
        XCTAssertNil(renewal["billing_timezone"])
        XCTAssertNil(request["expired_at"])
    }

    @MainActor
    func test_prerequisitesAndDisableAndClear() throws {
        let model = EditServerViewModel()
        model.prefill(from: try selectedConfig())
        XCTAssertNil(model.renewalValidationMessage)
        model.hasExpiry = false
        XCTAssertNotNil(model.renewalValidationMessage)
        model.hasExpiry = true
        model.billingCycle = ""
        XCTAssertNotNil(model.renewalValidationMessage)
        model.billingCycle = "weekly"
        XCTAssertNotNil(model.renewalValidationMessage)
        model.billingCycle = "yearly"
        model.billingTimezone = "Invalid/Zone"
        XCTAssertNotNil(model.renewalValidationMessage)
        model.billingTimezone = "America/Los_Angeles"
        model.automaticRenewal = false
        model.hasExpiry = false
        model.billingCycle = ""
        XCTAssertNil(model.renewalValidationMessage)
        let request = try object(model.buildRequest())
        let renewal = try XCTUnwrap(request["renewal"] as? [String: Any])
        XCTAssertEqual(renewal["enabled"] as? Bool, false)
        XCTAssertTrue(renewal["expiry_date"] is NSNull)
        XCTAssertTrue(request["billing_cycle"] is NSNull)
    }

    @MainActor
    func test_oldServerUnchangedSaveHasNoRenewalIntent() throws {
        let config = try JSONDecoder.snakeCase.decode(ServerConfig.self, from: Data("""
        {"id":"s1","name":"Server","expired_at":"2030-01-31T00:30:00Z"}
        """.utf8))
        let model = EditServerViewModel()
        model.prefill(from: config)
        XCTAssertFalse(model.supportsAutomaticRenewal)
        XCTAssertFalse(model.automaticRenewal)
        XCTAssertNil(try object(model.buildRequest())["renewal"])
        XCTAssertNil(try object(model.buildRequest())["expired_at"])
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
        {"id":"s1","name":"Server","billing_cycle":"monthly","expired_at":"2026-03-09T06:59:59Z",
        "renewal":{"enabled":true,"billing_timezone":"America/Los_Angeles","expiry_date":"2026-03-08",
        "confirmed_expired_at":"2026-02-09T07:59:59Z","deadline_origin":"projected","occurrence_id":"o2"}}
        """.utf8))
    }
}

extension RenewalDateTests {
    @MainActor
    func test_intervalOnlyEditOfClampedProjectionDoesNotSendConfirmation() throws {
        var config = try selectedConfig()
        config.renewal = ServerRenewal(
            enabled: true, billingTimezone: "America/Los_Angeles", expiryDate: "2026-02-28",
            confirmedExpiredAt: "2026-02-01T07:59:59Z", deadlineOrigin: "projected", occurrenceId: "february"
        )
        let model = EditServerViewModel()
        model.prefill(from: config)
        model.billingCycle = "quarterly"
        let request = try object(model.buildRequest())
        XCTAssertEqual(request["billing_cycle"] as? String, "quarterly")
        XCTAssertNil(request["renewal"])
        XCTAssertNil(request["expired_at"])
    }

    @MainActor
    func test_restoringClampedDateAndTimezoneLeavesOnlyUnrelatedEditIntent() throws {
        var config = try selectedConfig()
        config.renewal = ServerRenewal(
            enabled: true, billingTimezone: "America/Los_Angeles", expiryDate: "2026-02-28",
            confirmedExpiredAt: "2026-02-01T07:59:59Z", deadlineOrigin: "projected", occurrenceId: "february"
        )
        let model = EditServerViewModel()
        model.prefill(from: config)
        model.expiryDate = try XCTUnwrap(BillingDate.date(from: "2026-03-01", timezone: model.billingTimezone))
        model.expiryDate = try XCTUnwrap(BillingDate.date(from: "2026-02-28", timezone: model.billingTimezone))
        model.billingTimezone = "Asia/Tokyo"
        model.billingTimezone = "America/Los_Angeles"
        model.priceText = "15"
        let request = try object(model.buildRequest())
        XCTAssertEqual(request["price"] as? Double, 15)
        XCTAssertNil(request["renewal"])
        XCTAssertNil(request["expired_at"])
    }

    @MainActor
    func test_dateCorrectionAndTimezoneEditEncodeBothChangedFields() throws {
        let model = EditServerViewModel()
        model.prefill(from: try selectedConfig())
        model.billingTimezone = "Asia/Tokyo"
        model.expiryDate = try XCTUnwrap(BillingDate.date(from: "2028-02-29", timezone: model.billingTimezone))
        let request = try object(model.buildRequest())
        let renewal = try XCTUnwrap(request["renewal"] as? [String: Any])
        XCTAssertEqual(renewal["expiry_date"] as? String, "2028-02-29")
        XCTAssertEqual(renewal["billing_timezone"] as? String, "Asia/Tokyo")
        XCTAssertNil(renewal["enabled"])
        XCTAssertNil(request["expired_at"])
    }

    @MainActor
    func test_frozenDeadlineRemainsUnconfirmedOnSaveAndUsesExistingDateCorrection() throws {
        var config = try selectedConfig()
        config.renewal = ServerRenewal(
            enabled: false, billingTimezone: "America/Los_Angeles", expiryDate: "2026-02-28",
            confirmedExpiredAt: "2026-02-01T07:59:59Z", deadlineOrigin: "frozen", occurrenceId: "february"
        )
        let model = EditServerViewModel()
        model.prefill(from: config)
        XCTAssertFalse(model.automaticRenewal)
        XCTAssertNil(try object(model.buildRequest())["renewal"])
        model.expiryDate = try XCTUnwrap(BillingDate.date(from: "2026-02-15", timezone: model.billingTimezone))
        let request = try object(model.buildRequest())
        let renewal = try XCTUnwrap(request["renewal"] as? [String: Any])
        XCTAssertEqual(renewal["expiry_date"] as? String, "2026-02-15")
        XCTAssertNil(renewal["enabled"])
        XCTAssertNil(renewal["billing_timezone"])
        XCTAssertNil(request["expired_at"])
    }
}
