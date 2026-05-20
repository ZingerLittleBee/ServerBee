import XCTest
@testable import ServerBee

final class ISO8601ParserTests: XCTestCase {
    func test_parsesWithFractionalSeconds() {
        let date = ISO8601DateFormatter.shared.date(from: "2026-05-20T10:30:00.123Z")
        XCTAssertNotNil(date, "Must parse timestamps with fractional seconds")
    }

    func test_parsesWithoutFractionalSeconds() {
        let date = ISO8601DateFormatter.shared.date(from: "2026-05-20T10:30:00Z")
        XCTAssertNotNil(date, "Must parse timestamps without fractional seconds (chrono to_rfc3339 emits this when subsec is 0)")
    }

    func test_parsesWithTimezoneOffset() {
        let date = ISO8601DateFormatter.shared.date(from: "2026-05-20T10:30:00+00:00")
        XCTAssertNotNil(date, "Must parse timestamps with explicit timezone offset (chrono default)")
    }

    func test_returnsNilForGarbage() {
        XCTAssertNil(ISO8601DateFormatter.shared.date(from: "not-a-date"))
    }
}
