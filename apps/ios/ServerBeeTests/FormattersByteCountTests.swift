import XCTest
@testable import ServerBee

final class FormattersByteCountTests: XCTestCase {
    func test_formatBytes_zero() {
        // We set `allowsNonnumericFormatting = false` so a true 0 reads as the
        // numeric "0 bytes" rather than the locale word "Zero bytes" — clearer
        // in dense metric grids (e.g. "Disk I/O 0 bytes/s").
        XCTAssertEqual(Formatters.formatBytes(0), "0 bytes")
    }

    func test_formatBytes_oneKibibyte() {
        // ByteCountFormatter with .binary uses the unambiguous 1024 base
        // and "KB" label (per Apple's default countStyle = .file behaviour
        // which interprets KB as 1024). We assert "1 KB" because that's
        // exactly what ByteCountFormatter.string(fromByteCount:) emits at
        // en_US locale for 1024.
        XCTAssertEqual(Formatters.formatBytes(1024), "1 KB")
    }

    func test_formatBytes_oneMebibyte() {
        XCTAssertEqual(Formatters.formatBytes(1_048_576), "1 MB")
    }

    func test_formatBytes_belowOneKB() {
        // Under 1024 ByteCountFormatter emits bytes verbatim.
        XCTAssertEqual(Formatters.formatBytes(512), "512 bytes")
    }
}
