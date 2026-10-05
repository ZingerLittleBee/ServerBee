import XCTest
@testable import ServerBee

@MainActor
final class WebSocketRouterTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(AuthenticationURLProtocol.self) }

    override func tearDown() async throws {
        AuthenticationURLProtocol.handler = nil
        AuthenticationURLProtocol.cancelPending()
        URLProtocol.unregisterClass(AuthenticationURLProtocol.self)
        AuthManager().clearAuth()
    }

    func test_catalogRefreshUpdatesAuthenticatedReaders() async throws {
        let auth = AuthManager()
        auth.setServerUrl("https://catalog.test")
        auth.handleLoginResponse(MobileTokenResponse(
            accessToken: "catalog-access", accessExpiresInSecs: 900,
            refreshToken: "catalog-refresh", refreshExpiresInSecs: 3600,
            tokenType: "Bearer", user: MobileUser(id: "reader", username: "reader", role: "member")
        ))
        let api = APIClient(authManager: auth)
        let list = ServersViewModel()
        let detail = ServerDetailViewModel()
        let traffic = ServerTrafficViewModel()
        let insights = InsightsViewModel()
        list.handleWSMessage(.fullSync(servers: [ServerStatus(id: "renewing", name: "Old name", online: true, cpuUsage: 42)], upgrades: []))
        let log = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            XCTAssertEqual(request.request.value(forHTTPHeaderField: "Authorization"), "Bearer catalog-access")
            let body: String
            switch request.request.url?.path {
            case "/api/servers": body = #"{"data":[{"id":"renewing","name":"Current name"}]}"#
            case "/api/server-groups": body = #"{"data":[]}"#
            case "/api/servers/renewing":
                body = """
                {"data":{"id":"renewing","name":"Current name","expired_at":"2026-04-01T03:59:59Z",
                "renewal":{"enabled":true,"billing_timezone":"America/New_York","expiry_date":"2026-03-31",
                "confirmed_expired_at":"2026-02-01T04:59:59Z","deadline_origin":"projected","occurrence_id":"march"}}}
                """
            case "/api/servers/renewing/cost-insights":
                body = #"{"data":{"server_id":"renewing","configured":true,"advisories":[]}}"#
            case "/api/cost/overview":
                body = #"{"data":{"currencies":[],"servers":[{"server_id":"renewing","name":"Current name","configured":true,"advisories":[]}]}}"#
            default:
                XCTFail("Unexpected refresh endpoint")
                request.respond(404)
                return
            }
            request.respond(200, data: Data(body.utf8))
        }
        let router = WebSocketRouter(
            servers: { list.handleWSMessage($0) }, alerts: { _ in },
            catalogRefresh: { await list.refreshCatalog(serverIds: $0, apiClient: api) }
        )
        await router.dispatchAndRefresh(.serverCatalogChanged(serverIds: ["renewing"]))
        XCTAssertGreaterThan(list.catalogRevision(for: "renewing"), 0)
        XCTAssertEqual(list.catalogRevision(for: "unaffected"), 0)
        XCTAssertEqual(list.servers.first?.name, "Current name")
        XCTAssertEqual(list.servers.first?.cpuUsage, 42)
        XCTAssertEqual(list.servers.first?.online, true)

        // These are the same awaitable loaders used by revision-driven view tasks.
        await detail.fetchConfig(serverId: "renewing", apiClient: api)
        await traffic.refreshCost(serverId: "renewing", apiClient: api)
        await insights.refreshCost(apiClient: api)
        XCTAssertEqual(detail.config?.renewal?.expiryDate, "2026-03-31")
        XCTAssertEqual(detail.config?.renewal?.confirmedExpiredAt, "2026-02-01T04:59:59Z")
        XCTAssertEqual(traffic.cost?.advisories, [])
        XCTAssertEqual(insights.costOverview?.servers.first?.advisories, [])
        XCTAssertEqual(Set(log.snapshot().compactMap { $0.url?.path }), Set([
            "/api/servers", "/api/server-groups", "/api/servers/renewing",
            "/api/servers/renewing/cost-insights", "/api/cost/overview"
        ]))

        await router.dispatchAndRefresh(.fullSync(servers: list.servers, upgrades: []))
        XCTAssertGreaterThan(list.catalogRevision(for: "unaffected"), 0, "Reconnect refreshes every cached private reader")
        let revision = list.catalogRevision
        await list.refreshCatalog(serverIds: ["renewing"], apiClient: api)
        XCTAssertGreaterThan(list.catalogRevision, revision, "A local save refreshes the same readers without requiring a WS echo")
    }

    func test_catalogChangeRefreshesOnlyPrivateReaders() async throws {
        let message = try JSONDecoder.snakeCase.decode(BrowserMessage.self, from: Data(
            #"{"type":"server_catalog_changed","server_ids":["renewing"]}"#.utf8
        ))
        var refreshed: [[String]?] = []
        var servers: [BrowserMessage] = []
        let router = WebSocketRouter(
            servers: { servers.append($0) }, alerts: { _ in },
            catalogRefresh: { refreshed.append($0) }
        )
        await router.dispatchAndRefresh(message)
        XCTAssertEqual(refreshed, [["renewing"]])
        XCTAssertEqual(servers.count, 1)
        await router.dispatchAndRefresh(.update(servers: []))
        XCTAssertEqual(refreshed.count, 1, "Live updates must not refetch private billing data")
        await router.dispatchAndRefresh(.fullSync(servers: [], upgrades: []))
        XCTAssertEqual(refreshed.count, 2)
        XCTAssertNil(refreshed[1])
    }

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
