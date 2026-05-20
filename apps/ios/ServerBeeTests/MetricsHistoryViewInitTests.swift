import XCTest
@testable import ServerBee

@MainActor
final class MetricsHistoryViewInitTests: XCTestCase {
    func testRecordsPathUsesBackendFromToContract() throws {
        let query = MetricsHistoryQuery(range: "6h", now: Date(timeIntervalSince1970: 3_600 * 12))
        let path = query.path(serverId: "srv-1")
        let components = try XCTUnwrap(URLComponents(string: "https://example.test\(path)"))
        var items: [String: String] = [:]
        for item in components.queryItems ?? [] {
            items[item.name] = item.value
        }

        XCTAssertEqual(components.path, "/api/servers/srv-1/records")
        XCTAssertEqual(items["interval"], "raw")
        XCTAssertEqual(items["from"], "1970-01-01T06:00:00Z")
        XCTAssertEqual(items["to"], "1970-01-01T12:00:00Z")
        XCTAssertNil(items["range"])
    }

    func testSevenDayRecordsPathUsesHourlyInterval() throws {
        let query = MetricsHistoryQuery(range: "7d", now: Date(timeIntervalSince1970: 3_600 * 24 * 10))
        let path = query.path(serverId: "srv-1")
        let components = try XCTUnwrap(URLComponents(string: "https://example.test\(path)"))
        let interval = components.queryItems?.first { $0.name == "interval" }?.value

        XCTAssertEqual(interval, "hourly")
    }

    func testAppearanceViewDoesNotRenderLanguageSection() throws {
        let source = try String(
            contentsOfFile: #filePath
                .replacingOccurrences(of: "ServerBeeTests/MetricsHistoryViewInitTests.swift",
                                      with: "ServerBee/Views/Settings/AppearanceView.swift")
        )
        XCTAssertFalse(
            source.contains("String(localized: \"Language\")"),
            "Appearance must only expose Theme; language selection belongs to iOS Settings"
        )
    }

    func testViewDoesNotConstructItsOwnAPIClient() throws {
        let source = try String(
            contentsOfFile: #filePath
                .replacingOccurrences(of: "ServerBeeTests/MetricsHistoryViewInitTests.swift",
                                      with: "ServerBee/Views/Servers/MetricsHistoryView.swift")
        )
        XCTAssertFalse(
            source.contains("APIClient(authManager:"),
            "MetricsHistoryView must consume APIClient from the environment, not construct one"
        )
    }
}
