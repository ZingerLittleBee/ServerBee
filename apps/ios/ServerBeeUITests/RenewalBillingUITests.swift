import XCTest

/// Real SwiftUI controls and production request encoding against a local HTTP
/// seam. Calendar recurrence and persistence are verified by Server HTTP tests.
@MainActor
final class RenewalBillingUITests: XCTestCase {
    private let app = XCUIApplication()
    private var fixtureURL: URL?

    func testBillingDateAndTimezoneRemainIndependentOfDeviceTimezone() async throws {
        defer { screenshot("timezone-final"); app.terminate() }
        try await launch(scenario: "timezone")
        let deviceZone = reveal("renewal.deviceTimezone")
        let zone = deviceZone.label
        XCTAssertFalse(zone.isEmpty, "Missing actual application TimeZone.current readout")
        XCTAssertNotEqual(zone, "America/New_York", "This acceptance requires a device/billing timezone mismatch")
        let metadata = XCTAttachment(string: "Application TimeZone.current.identifier: \(zone)")
        metadata.name = "actual-device-timezone"
        metadata.lifetime = .keepAlways
        add(metadata)
        assertDate("Jan 31, 2026")
        assertText("renewal.timezone", contains: "America/New_York")
        screenshot("stored-january-date")

        reveal("renewal.timezone").tap()
        XCTAssertTrue(app.navigationBars["Billing timezone"].waitForExistence(timeout: 5), app.debugDescription)
        let search = app.searchFields.matching(NSPredicate(format: "placeholderValue == %@", "Search billing timezones")).firstMatch
        XCTAssertTrue(search.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(search.isHittable, search.debugDescription)
        search.tap()
        search.typeText("UTC")
        let utc = app.buttons["UTC"].firstMatch
        XCTAssertTrue(utc.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(utc.isHittable, app.debugDescription)
        screenshot("timezone-search-utc")
        utc.tap()
        XCTAssertTrue(app.navigationBars["Edit Server"].waitForExistence(timeout: 5), app.debugDescription)
        assertText("renewal.timezone", contains: "UTC")
        assertDate("Jan 31, 2026")
        screenshot("utc-picker-selected-date-preserved")
        try await save(expectedCount: 1)
        let body = try await capturedSave(index: 0)
        XCTAssertEqual(body["renewal"] as? [String: String], ["billing_timezone": "UTC"])
        XCTAssertNil(body["expired_at"])
        try await reopen(expectedOrigin: "Confirmed renewal deadline")
        assertText("renewal.timezone", contains: "UTC")
        assertDate("Jan 31, 2026")
        screenshot("reopened-utc-date")
        try await assertFixtureHealthy()
    }

    func testAutomaticRenewalCanBeEnabledThenFrozenThroughControls() async throws {
        defer { screenshot("switch-final"); app.terminate() }
        try await launch(scenario: "switch")
        assertSwitch(enabled: false)
        assertDate("Jan 31, 2026")
        tapRenewalSwitch()
        assertSwitch(enabled: true)
        screenshot("enable-selected")
        try await save(expectedCount: 1)
        let enabledBody = try await capturedSave(index: 0)
        XCTAssertEqual(enabledBody["renewal"] as? [String: Bool], ["enabled": true])
        XCTAssertNil(enabledBody["expired_at"])
        try await reopen(expectedOrigin: "Projected renewal deadline")
        assertSwitch(enabled: true)
        assertDate("Feb 28, 2026")
        assertExplanation("This deadline is a forecast. It does not confirm provider renewal or payment.")
        screenshot("saved-projected-deadline")
        let projected = try await fixtureState()
        let projectedRenewal = try renewal(in: projected)

        tapRenewalSwitch()
        assertSwitch(enabled: false)
        try await save(expectedCount: 2)
        let disabledBody = try await capturedSave(index: 1)
        XCTAssertEqual(disabledBody["renewal"] as? [String: Bool], ["enabled": false])
        XCTAssertNil(disabledBody["expired_at"])
        try await reopen(expectedOrigin: "Frozen renewal deadline")
        assertSwitch(enabled: false)
        assertDate("Feb 28, 2026")
        assertText("renewal.confirmed", contains: "Jan 31, 2026")
        assertExplanation("Automatic renewal is off. This forecast is frozen; it does not confirm provider renewal or payment.")
        let frozenRenewal = try renewal(in: await fixtureState())
        XCTAssertEqual(frozenRenewal["occurrence_id"] as? String, projectedRenewal["occurrence_id"] as? String)
        XCTAssertEqual(frozenRenewal["confirmed_expired_at"] as? String, projectedRenewal["confirmed_expired_at"] as? String)
        screenshot("saved-frozen-deadline-and-history")
        try await assertFixtureHealthy()
    }

    func testManualDateEditUsesTheSelectedBillingCalendarDate() async throws {
        defer { screenshot("manual-final"); app.terminate() }
        try await launch(scenario: "manual")
        assertSwitch(enabled: true)
        assertDate("Feb 28, 2026")
        reveal("renewal.expiry").tap()
        // Compact DatePicker opens Apple's calendar. Select an actual day cell,
        // never a binding, view-model method or fixture date injection.
        let fullDate = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "February 15")).firstMatch
        let day = app.buttons["15"].firstMatch
        let selectedDay: XCUIElement
        if fullDate.waitForExistence(timeout: 3) {
            selectedDay = fullDate
        } else {
            XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "February 2026")).firstMatch.exists,
                          "Expected February calendar: \(app.debugDescription)")
            XCTAssertTrue(day.waitForExistence(timeout: 3), app.debugDescription)
            selectedDay = day
        }
        screenshot("manual-calendar-before-day-selection")
        selectedDay.tap()
        // UIKit labels the day Button with its full date; "15" is a child
        // StaticText. If the calendar stays open, a real tap on the editor
        // title outside that popover dismisses it without changing the date.
        if selectedDay.exists {
            let navigation = app.navigationBars["Edit Server"]
            XCTAssertTrue(navigation.exists, app.debugDescription)
            navigation.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
            XCTAssertFalse(selectedDay.exists, "Calendar must dismiss after the outside tap: \(app.debugDescription)")
        }
        assertDate("Feb 15, 2026")
        screenshot("manual-february-fifteenth-selected")
        try await save(expectedCount: 1)
        let dateBody = try await capturedSave(index: 0)
        XCTAssertEqual(dateBody["renewal"] as? [String: String], ["expiry_date": "2026-02-15"])
        XCTAssertNil(dateBody["expired_at"])
        try await reopen(expectedOrigin: "Projected renewal deadline")
        assertDate("Feb 15, 2026")
        assertText("renewal.timezone", contains: "America/New_York")
        assertText("renewal.confirmed", contains: "Feb 15, 2026")
        screenshot("manual-saved-projection-and-new-history")

        let name = reveal("server.name", scrollingDown: false)
        name.tap()
        if let value = name.value as? String {
            name.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: value.count))
        }
        name.typeText("Renamed renewal fixture")
        try await save(expectedCount: 2)
        let nameBody = try await capturedSave(index: 1)
        XCTAssertEqual(nameBody["name"] as? String, "Renamed renewal fixture")
        XCTAssertNil(nameBody["renewal"])
        XCTAssertNil(nameBody["expired_at"])
        try await reopen(expectedOrigin: "Projected renewal deadline")
        assertDate("Feb 15, 2026")
        assertText("renewal.confirmed", contains: "Feb 15, 2026")
        assertText("renewal.timezone", contains: "America/New_York")
        screenshot("name-only-save-preserves-date-and-origin")
        try await assertFixtureHealthy()
    }
}

private extension RenewalBillingUITests {
    enum FixtureError: Error { case invalidURL, invalidResponse, invalidState }

    func launch(scenario: String) async throws {
        continueAfterFailure = false
        let raw = ProcessInfo.processInfo.environment["SERVERBEE_RENEWAL_FIXTURE_URL"] ?? ""
        guard let url = URL(string: raw), url.scheme == "http", url.host == "127.0.0.1", url.port != nil,
              url.path.isEmpty, url.query == nil, url.user == nil else { throw FixtureError.invalidURL }
        fixtureURL = url
        _ = try await request("/__test/reset", body: ["scenario": scenario])
        let sessions = [
            "switch": "11111111-1111-4111-8111-111111111111",
            "timezone": "22222222-2222-4222-8222-222222222222",
            "manual": "33333333-3333-4333-8333-333333333333"
        ]
        guard let session = sessions[scenario] else { throw FixtureError.invalidState }
        app.launchArguments = ["-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launchEnvironment = [
            "SB_UITEST_SERVER": raw,
            "SB_UITEST_ACCESS": "fixture-renewal-access",
            "SB_UITEST_REFRESH": "fixture-renewal-refresh",
            "SB_UITEST_REVOCATION": "fixture-renewal-deletion",
            "SB_UITEST_MOBILE_SESSION_ID": session,
            "SB_UITEST_INSTALLATION_ID": "renewal-ui-installation-\(scenario)",
            "SB_UITEST_USERNAME": "fixture-admin", "SB_UITEST_USER_ID": "fixture-user",
            "SB_UITEST_ROLE": "admin", "SB_UITEST_DEEPLINK": "server:renewal-ui-server",
            "SB_UITEST_PRESENT": "edit-server", "SB_UITEST_RENEWAL_CONTEXT": "1"
        ]
        app.launch()
        XCTAssertTrue(app.buttons["server.save"].waitForExistence(timeout: 20), app.debugDescription)
        XCTAssertTrue(app.textFields["server.name"].waitForExistence(timeout: 5))
        // Groups/tags load asynchronously during form prefill. Wait for those
        // real auxiliary reads before enabling save interactions.
        for _ in 0..<50 {
            let state = try await fixtureState()
            let reads = state["reads"] as? [String: Int] ?? [:]
            if reads["/api/server-groups", default: 0] > 0 && reads["/api/servers/renewal-ui-server/tags", default: 0] > 0 {
                return
            }
            try await Task.sleep(for: .milliseconds(100))
        }
        XCTFail("Editor groups/tags reads did not complete")
    }

    func element(_ identifier: String) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: identifier).firstMatch
    }

    @discardableResult
    func reveal(_ identifier: String, scrollingDown: Bool = true) -> XCUIElement {
        let target = element(identifier)
        for _ in 0..<14 {
            if target.exists && target.isHittable { return target }
            if scrollingDown { app.swipeUp() } else { app.swipeDown() }
        }
        XCTAssertTrue(target.exists && target.isHittable, "Control \(identifier): \(app.debugDescription)")
        return target
    }

    func assertText(_ identifier: String, contains text: String) {
        let target = reveal(identifier)
        let predicate = NSPredicate(format: "label CONTAINS %@ OR value CONTAINS %@", text, text)
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: target)
        if XCTWaiter.wait(for: [expectation], timeout: 5) != .completed {
            XCTAssertTrue(target.descendants(matching: .any).matching(predicate).firstMatch.exists,
                          "Expected \(text) in \(target.debugDescription)")
        }
    }

    func assertDate(_ text: String) {
        assertText("renewal.expiry", contains: text)
    }

    func assertSwitch(enabled: Bool) {
        let toggle = reveal("renewal.enabled")
        XCTAssertEqual(toggle.value as? String, enabled ? "1" : "0", toggle.debugDescription)
    }

    func tapRenewalSwitch() {
        // SwiftUI exposes both the full labelled row and its physical switch
        // as Switch elements. A tap at the row center hits the label, whereas
        // the actual control is the small trailing descendant switch.
        let row = reveal("renewal.enabled")
        let control = row.switches.firstMatch
        XCTAssertTrue(control.exists && control.isHittable, row.debugDescription)
        control.tap()
    }

    func assertExplanation(_ text: String) {
        let target = app.staticTexts[text].firstMatch
        for _ in 0..<8 {
            if target.exists && target.isHittable { break }
            app.swipeUp()
        }
        XCTAssertTrue(target.exists, app.debugDescription)
    }

    func screenshot(_ name: String) {
        let image = app.state == .runningForeground ? app.screenshot() : XCUIScreen.main.screenshot()
        let attachment = XCTAttachment(screenshot: image)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    func save(expectedCount: Int) async throws {
        app.buttons["server.save"].tap()
        let edit = app.buttons["server.edit"]
        XCTAssertTrue(edit.waitForExistence(timeout: 10), app.debugDescription)
        waitUntilHittable(edit)
        let state = try await fixtureState()
        XCTAssertEqual((state["saves"] as? [[String: Any]])?.count, expectedCount)
        XCTAssertFalse(app.buttons["server.save"].exists, "Editor must dismiss after a real save")
    }

    func waitUntilHittable(_ target: XCUIElement) {
        let ready = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hittable == true"), object: target)
        XCTAssertEqual(XCTWaiter.wait(for: [ready], timeout: 10), .completed, app.debugDescription)
    }

    func reopen(expectedOrigin: String) async throws {
        // Await the detail's real post-save GET (not merely the PUT response).
        // Opening the production toolbar immediately could observe old config.
        let origin = app.staticTexts[expectedOrigin].firstMatch
        for _ in 0..<10 {
            if origin.exists { break }
            app.swipeUp()
        }
        XCTAssertTrue(origin.waitForExistence(timeout: 10), app.debugDescription)
        app.buttons["server.edit"].tap()
        XCTAssertTrue(app.buttons["server.save"].waitForExistence(timeout: 10))
        assertText("renewal.origin", contains: expectedOrigin)
    }

    func request(_ path: String, body: [String: String]? = nil) async throws -> [String: Any] {
        guard let base = fixtureURL else { throw FixtureError.invalidURL }
        var request = URLRequest(url: base.appendingPathComponent(path))
        request.timeoutInterval = 5
        if let body {
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        }
        let (data, response) = try await URLSession.shared.data(for: request)
        guard (response as? HTTPURLResponse)?.statusCode == 200,
              let result = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw FixtureError.invalidResponse
        }
        return result
    }

    func fixtureState() async throws -> [String: Any] {
        try await request("/__test/state")
    }

    func capturedSave(index: Int) async throws -> [String: Any] {
        let state = try await fixtureState()
        guard let saves = state["saves"] as? [[String: Any]], saves.indices.contains(index) else {
            throw FixtureError.invalidState
        }
        return saves[index]
    }

    func renewal(in state: [String: Any]) throws -> [String: Any] {
        guard let server = state["server"] as? [String: Any],
              let renewal = server["renewal"] as? [String: Any] else { throw FixtureError.invalidState }
        return renewal
    }

    func assertFixtureHealthy() async throws {
        let state = try await fixtureState()
        XCTAssertEqual(state["errors"] as? [String], [], "Unknown routes or invalid intent must fail acceptance")
    }
}
