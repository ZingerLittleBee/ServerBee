import XCTest
@testable import ServerBee

/// Verifies that `ServersViewModel.fetchServers` surfaces failures via
/// `errorMessage` and clears it on success. Failure path is induced by
/// pointing the `APIClient` at an `AuthManager` with no `serverUrl`, which
/// causes `performRequest` to throw `APIError.noServerUrl`.
@MainActor
final class ServersViewModelErrorTests: XCTestCase {
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

    func test_fetchServers_setsErrorMessage_onFailure() async {
        let viewModel = ServersViewModel()
        let auth = AuthManager()
        auth.serverUrl = nil
        let client = APIClient(authManager: auth)

        await viewModel.fetchServers(apiClient: client)

        XCTAssertNotNil(viewModel.errorMessage)
        XCTAssertTrue(viewModel.servers.isEmpty)
    }

    func test_fetchServers_clearsErrorMessage_afterFailedRequestThenSuccessfulRetry() async {
        let viewModel = ServersViewModel()
        viewModel.servers = [ServerStatus(id: "1", name: "Cached", online: true)]
        let auth = AuthManager()
        auth.setServerUrl("https://servers.test")
        auth.handleLoginResponse(MobileTokenResponse(
            accessToken: "fixture-access", accessExpiresInSecs: 900,
            refreshToken: "fixture-refresh", refreshExpiresInSecs: 3_600,
            tokenType: "Bearer", user: MobileUser(id: "alice", username: "alice", role: "member"),
            revocationToken: "fixture-proof", mobileSessionId: "11111111-1111-4111-8111-111111111111"
        ))
        let client = APIClient(authManager: auth)
        AuthenticationURLProtocol.handler = { request in
            if request.request.url?.path == "/api/servers" {
                request.respond(503, data: Data(#"{"error":{"message":"Unavailable"}}"#.utf8))
            } else {
                request.respond(200, data: Data(#"{"data":[]}"#.utf8))
            }
        }

        await viewModel.fetchServers(apiClient: client)
        XCTAssertNotNil(viewModel.errorMessage)
        XCTAssertEqual(viewModel.servers.first?.name, "Cached")

        AuthenticationURLProtocol.handler = { request in
            if request.request.url?.path == "/api/servers" {
                request.respond(200, data: Data(#"{"data":[{"id":"1","name":"Refreshed","group_id":"g1"}]}"#.utf8))
            } else {
                request.respond(200, data: Data(#"{"data":[{"id":"g1","name":"Production"}]}"#.utf8))
            }
        }
        await viewModel.fetchServers(apiClient: client)

        XCTAssertNil(viewModel.errorMessage)
        XCTAssertEqual(viewModel.servers.count, 1)
        XCTAssertEqual(viewModel.servers.first?.name, "Refreshed")
        XCTAssertEqual(viewModel.servers.first?.online, true)
        XCTAssertEqual(viewModel.groupsByID, ["g1": "Production"])
        XCTAssertFalse(viewModel.isLoading)
    }
}
