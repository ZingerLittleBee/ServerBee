import XCTest
@testable import ServerBee

final class TrafficChartSupportTests: XCTestCase {
    private let mib = 1_048_576.0
    private let gib = 1_073_741_824.0

    // MARK: TrafficAxisScale

    func test_axisScale_emptyHistory() {
        let scale = TrafficAxisScale(maxBytes: 0)
        XCTAssertEqual(scale.ticks, [0])
        XCTAssertEqual(scale.label(for: 0), "0")
    }

    func test_axisScale_movesUpAUnitInsteadOfAFourDigitTopTick() {
        let scale = TrafficAxisScale(maxBytes: Int64(900 * mib))
        XCTAssertEqual(scale.ticks.map(scale.label(for:)), ["0", "0.5 GB", "1 GB"])
        XCTAssertGreaterThanOrEqual(scale.upperBound, 900 * mib)
    }

    func test_axisScale_keepsRoundTicksInOneUnit() {
        let scale = TrafficAxisScale(maxBytes: Int64(150 * gib))
        XCTAssertEqual(scale.ticks.map(scale.label(for:)), ["0", "100 GB", "200 GB"])
    }

    func test_axisScale_quarterStepsKeepTwoDecimals() {
        // 0.49 GiB: half of it needs a 0.25 GB step.
        let scale = TrafficAxisScale(maxBytes: Int64(0.49 * gib))
        XCTAssertEqual(scale.ticks.map(scale.label(for:)), ["0", "0.25 GB", "0.5 GB"])
    }

    // MARK: TrafficDayFormat.rangeCaption

    private func now() throws -> Date {
        try XCTUnwrap(Formatters.parseDay("2026-09-28")).addingTimeInterval(3600 * 10)
    }

    func test_rangeCaption_recentHistoryCountsCalendarDays() throws {
        // Rows with gaps still describe the whole span, not the row count.
        let caption = TrafficDayFormat.rangeCaption(days: ["2026-09-18", "2026-09-22", "2026-09-27"], now: try now())
        XCTAssertEqual(caption, String(localized: "last \(10) days"))
    }

    func test_rangeCaption_singleRecentDay() throws {
        XCTAssertEqual(TrafficDayFormat.rangeCaption(days: ["2026-09-28"], now: try now()), String(localized: "last 1 day"))
    }

    func test_rangeCaption_stoppedHistoryNamesItsLastDay() throws {
        let caption = TrafficDayFormat.rangeCaption(days: ["2026-08-01", "2026-09-03"], now: try now())
        let last = try XCTUnwrap(Formatters.parseDay("2026-09-03"))
        XCTAssertEqual(caption, String(localized: "through \(TrafficDayFormat.short(last))"))
    }

    func test_rangeCaption_emptyHistoryHasNoCaption() throws {
        XCTAssertNil(TrafficDayFormat.rangeCaption(days: [], now: try now()))
    }
}
