import XCTest
@testable import ServerBee

/// Unit-tests the pure transformation `ServerDeepLink → (tab, path)` used by
/// `ContentView.handleDeepLink`. The mapping itself is the contract we care
/// about; full SwiftUI navigation is verified by the UI smoke test in Task 11.
///
/// Tests invoke `ContentView.applyDeepLink` directly — the same static helper
/// `handleDeepLink` delegates to — so any divergence is caught immediately
/// without maintaining a mirror in the test file.
@MainActor
final class DeepLinkNavigationTests: XCTestCase {
    func test_serverDetailLink_setsServersTabAndPath() {
        var selectedTab = 99
        var serversPath: [ServerNavigationTarget] = []
        var alertsPath: [ServerDeepLink] = []

        ContentView.applyDeepLink(
            .serverDetail(serverId: "srv-abc"),
            selectedTab: &selectedTab,
            serversPath: &serversPath,
            alertsPath: &alertsPath
        )

        XCTAssertEqual(selectedTab, 0)
        XCTAssertEqual(serversPath, [.detailById("srv-abc")])
        XCTAssertTrue(alertsPath.isEmpty)
    }

    func test_alertDetailLink_setsAlertsTabAndPath() {
        var selectedTab = 99
        var serversPath: [ServerNavigationTarget] = []
        var alertsPath: [ServerDeepLink] = []

        ContentView.applyDeepLink(
            .alertDetail(alertKey: "rule-7"),
            selectedTab: &selectedTab,
            serversPath: &serversPath,
            alertsPath: &alertsPath
        )

        XCTAssertEqual(selectedTab, 1)
        XCTAssertEqual(alertsPath, [.alertDetail(alertKey: "rule-7")])
        XCTAssertTrue(serversPath.isEmpty)
    }
}
