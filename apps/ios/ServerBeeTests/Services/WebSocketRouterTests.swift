import XCTest
@testable import ServerBee

@MainActor
final class WebSocketRouterTests: XCTestCase {
    func test_alertEvent_invokesAlertHandlerOnly() async {
        var servers: [BrowserMessage] = []
        var alerts: [BrowserMessage] = []
        let router = WebSocketRouter(
            servers: { servers.append($0) },
            alerts: { alerts.append($0) }
        )

        router.dispatch(.alertEvent(alertKey: "k", status: .firing))
        XCTAssertEqual(servers.count, 0)
        XCTAssertEqual(alerts.count, 1)
    }

    func test_serverUpdate_invokesServersHandlerOnly() async {
        var servers: [BrowserMessage] = []
        var alerts: [BrowserMessage] = []
        let router = WebSocketRouter(
            servers: { servers.append($0) },
            alerts: { alerts.append($0) }
        )

        router.dispatch(.update(servers: []))
        XCTAssertEqual(servers.count, 1)
        XCTAssertEqual(alerts.count, 0)
    }
}
