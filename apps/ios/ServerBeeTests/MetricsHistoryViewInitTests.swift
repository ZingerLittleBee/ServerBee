import XCTest
@testable import ServerBee

@MainActor
final class MetricsHistoryViewInitTests: XCTestCase {
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
