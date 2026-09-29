import XCTest
@testable import ServerBee

final class FormattersByteCountTests: XCTestCase {
    func test_formatBytes_zeroAndBytesUseTheBUnit() {
        // No locale words ("0 bytes" / "0 字节"): they overflow dense metric rows.
        XCTAssertEqual(Formatters.formatBytes(0), "0 B")
        XCTAssertEqual(Formatters.formatBytes(512), "512 B")
    }

    func test_formatBytes_binaryUnits() {
        XCTAssertEqual(Formatters.formatBytes(1024), "1 KB")
        XCTAssertEqual(Formatters.formatBytes(1_048_576), "1 MB")
        XCTAssertEqual(Formatters.formatBytes(1_572_864), "1.5 MB")
        XCTAssertEqual(Formatters.formatBytes(20 * 1_073_741_824), "20 GB")
    }

    func test_formatBytes_rollsOverBeforeFourDigits() {
        XCTAssertEqual(Formatters.formatBytes(1010 * 1024), "1 MB")
        XCTAssertEqual(Formatters.formatBytes(999 * 1024), "999 KB")
        XCTAssertEqual(Formatters.formatBytes(1000), "1 KB")
    }

    func test_formatBytes_clampsNegativeToZero() {
        XCTAssertEqual(Formatters.formatBytes(-5), "0 B")
    }

    func test_formatSpeed_appendsPerSecondAndDashesMissingValues() {
        XCTAssertEqual(Formatters.formatSpeed(0), "0 B/s")
        XCTAssertEqual(Formatters.formatSpeed(5 * 1_048_576), "5 MB/s")
        XCTAssertEqual(Formatters.formatSpeed(nil), "—")
    }

    func test_formatPercentage_dashesMissingValues() {
        XCTAssertEqual(Formatters.formatPercentage(42.26), "42.3%")
        XCTAssertEqual(Formatters.formatPercentage(nil), "—")
    }

    func test_serverListRatePair_sharesTheLargerUnit() {
        XCTAssertEqual(ServerListRateFormat.pair(down: 1010 * 1024, up: 300 * 1024), "↓1 ↑0.3 MB/s")
        XCTAssertEqual(ServerListRateFormat.pair(down: 0, up: 0), "↓0 ↑0 B/s")
    }
}
