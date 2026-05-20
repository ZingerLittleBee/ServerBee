import XCTest
@testable import ServerBee

final class FormattersRelativeTimeTests: XCTestCase {
    func test_formatRelativeTime_returnsNonEmptyForRecentTimestamp() {
        // 30 seconds ago
        let date = Date().addingTimeInterval(-30)
        let iso = ISO8601DateFormatter.shared.string(from: date)
        let result = Formatters.formatRelativeTime(iso)
        XCTAssertFalse(result.isEmpty)
        // We don't assert the exact wording (it's locale-dependent and
        // RelativeDateTimeFormatter wording can shift between iOS versions),
        // only that we got a non-empty localized string back rather than
        // the raw ISO timestamp.
        XCTAssertFalse(result.contains("T"), "Should not echo back the raw ISO timestamp")
        XCTAssertFalse(result.contains("Z"), "Should not echo back the raw ISO timestamp")
    }

    func test_formatRelativeTime_returnsOriginalOnParseFailure() {
        let garbage = "not-a-date"
        XCTAssertEqual(Formatters.formatRelativeTime(garbage), garbage)
    }
}
