import XCTest
@testable import ServerBee

@MainActor
final class ServerDetailViewModelTests: XCTestCase {
    override func setUp() async throws {
        URLProtocol.registerClass(AuthenticationURLProtocol.self)
    }

    override func tearDown() async throws {
        AuthenticationURLProtocol.handler = nil
        AuthenticationURLProtocol.cancelPending()
        URLProtocol.unregisterClass(AuthenticationURLProtocol.self)
        AuthManager().clearAuth()
        try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
    }

    private func client() -> APIClient {
        let auth = AuthManager()
        auth.setServerUrl("https://metrics.test")
        auth.handleLoginResponse(MobileTokenResponse(
            accessToken: "fixture-access", accessExpiresInSecs: 900,
            refreshToken: "fixture-refresh", refreshExpiresInSecs: 3_600,
            tokenType: "Bearer", user: MobileUser(id: "alice", username: "alice", role: "member"),
            revocationToken: "fixture-proof", mobileSessionId: "11111111-1111-4111-8111-111111111111"
        ))
        return APIClient(authManager: auth)
    }

    func testLatestRangeRemainsVisibleWhenOlderRequestFinishesLast() async throws {
        let viewModel = ServerDetailViewModel()
        let api = client()
        let olderStarted = expectation(description: "older range request started")
        let older = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in
            if request.request.url?.query?.contains("interval=raw") == true {
                older.hold(request)
                olderStarted.fulfill()
            } else {
                request.respond(200, data: Data(#"{"data":[{"time":"2026-10-18T00:00:00Z","cpu":7}]}"#.utf8))
            }
        }

        let oldFetch = Task { await viewModel.fetchRecords(serverId: "srv-1", range: "1h", apiClient: api) }
        await fulfillment(of: [olderStarted], timeout: 3)
        await viewModel.fetchRecords(serverId: "srv-1", range: "7d", apiClient: api)
        XCTAssertEqual(viewModel.records.first?.cpuUsage, 7)

        try XCTUnwrap(older.takeHeld()).respond(200, data: Data(#"{"data":[{"time":"2026-10-18T00:00:00Z","cpu":1}]}"#.utf8))
        await oldFetch.value

        XCTAssertEqual(viewModel.records.first?.cpuUsage, 7, "A stale range response must not replace the selected range")
        XCTAssertFalse(viewModel.isLoading)
    }

    func testOlderCompletionKeepsLatestRangeLoading() async throws {
        let viewModel = ServerDetailViewModel()
        let api = client()
        let olderStarted = expectation(description: "older range request started")
        let latestStarted = expectation(description: "latest range request started")
        let older = AuthenticationRequestLog()
        let latest = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in
            if request.request.url?.query?.contains("interval=raw") == true {
                older.hold(request)
                olderStarted.fulfill()
            } else {
                latest.hold(request)
                latestStarted.fulfill()
            }
        }

        let oldFetch = Task { await viewModel.fetchRecords(serverId: "srv-1", range: "1h", apiClient: api) }
        await fulfillment(of: [olderStarted], timeout: 3)
        let latestFetch = Task { await viewModel.fetchRecords(serverId: "srv-1", range: "7d", apiClient: api) }
        await fulfillment(of: [latestStarted], timeout: 3)

        try XCTUnwrap(older.takeHeld()).respond(200, data: Data(#"{"data":[{"time":"2026-10-18T00:00:00Z","cpu":1}]}"#.utf8))
        await oldFetch.value
        XCTAssertTrue(viewModel.isLoading, "Only the latest request may clear its loading indicator")
        XCTAssertTrue(viewModel.records.isEmpty, "An obsolete response must not display another range's records")

        try XCTUnwrap(latest.takeHeld()).respond(200, data: Data(#"{"data":[{"time":"2026-10-18T00:00:00Z","cpu":7}]}"#.utf8))
        await latestFetch.value
        XCTAssertEqual(viewModel.records.first?.cpuUsage, 7)
        XCTAssertFalse(viewModel.isLoading)
    }

    func testAlreadyCancelledRequestDoesNotInvalidateLatestRange() async throws {
        let viewModel = ServerDetailViewModel()
        let api = client()
        let latestStarted = expectation(description: "latest range request started")
        let latest = AuthenticationRequestLog()
        let requests = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in
            _ = requests.append(request.request)
            if request.request.url?.query?.contains("interval=hourly") == true {
                latest.hold(request)
                latestStarted.fulfill()
            } else {
                request.respond(200, data: Data(#"{"data":[]}"#.utf8))
            }
        }

        let latestFetch = Task { await viewModel.fetchRecords(serverId: "srv-1", range: "7d", apiClient: api) }
        await fulfillment(of: [latestStarted], timeout: 3)
        let cancelledFetch = Task { await viewModel.fetchRecords(serverId: "srv-1", range: "1h", apiClient: api) }
        cancelledFetch.cancel()
        await cancelledFetch.value

        XCTAssertTrue(viewModel.isLoading, "A request cancelled before entry cannot own the latest loading state")
        XCTAssertEqual(requests.snapshot().count, 1, "A request cancelled before entry must not send another HTTP request")
        try XCTUnwrap(latest.takeHeld()).respond(200, data: Data(#"{"data":[{"time":"2026-10-18T00:00:00Z","cpu":7}]}"#.utf8))
        await latestFetch.value

        XCTAssertEqual(viewModel.records.first?.cpuUsage, 7)
        XCTAssertFalse(viewModel.isLoading)
    }
}
