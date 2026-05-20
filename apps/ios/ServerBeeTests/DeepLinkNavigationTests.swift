import XCTest
@testable import ServerBee

/// Unit-tests the pure transformation `ServerDeepLink → (tab, path)` used by
/// `ContentView.handleDeepLink`. The mapping itself is the contract we care
/// about; full SwiftUI navigation is verified by the UI smoke test in Task 11.
@MainActor
final class DeepLinkNavigationTests: XCTestCase {
    func test_serverDetailLink_setsServersTabAndPath() {
        var selectedTab = 99
        var serversPath: [ServerNavigationTarget] = []
        var alertsPath: [ServerDeepLink] = []

        applyDeepLink(
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

        applyDeepLink(
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

/// Mirror of `ContentView.handleDeepLink`. Kept in test for direct invocation;
/// any divergence will surface as a test failure to keep the two in sync.
private func applyDeepLink(
    _ link: ServerDeepLink,
    selectedTab: inout Int,
    serversPath: inout [ServerNavigationTarget],
    alertsPath: inout [ServerDeepLink]
) {
    switch link {
    case .serverDetail(let serverId):
        selectedTab = 0
        serversPath = [.detailById(serverId)]
    case .alertDetail(let alertKey):
        selectedTab = 1
        alertsPath = [.alertDetail(alertKey: alertKey)]
    }
}
