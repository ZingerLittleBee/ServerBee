import XCTest
@testable import ServerBee

/// Stubs pair-flow HTTP traffic via the shared `URLProtocolStub` already
/// available in this test target (defined in `RefreshErrorClassificationTests.swift`).
@MainActor
final class AuthViewModelPairTests: XCTestCase {
    private var session: URLSession!

    override func setUp() async throws {
        URLProtocol.registerClass(URLProtocolStub.self)
        URLProtocolStub.stubResponse = nil
        URLProtocolStub.stubError = nil
        URLProtocolStub.stubResponseFactory = nil
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [URLProtocolStub.self]
        session = URLSession(configuration: config)
    }

    override func tearDown() async throws {
        URLProtocol.unregisterClass(URLProtocolStub.self)
        URLProtocolStub.stubResponse = nil
        URLProtocolStub.stubError = nil
        URLProtocolStub.stubResponseFactory = nil
        session = nil
    }

    func test_pair_returnsToken_andHydratesAuthManager_on200() async throws {
        let payload = """
        {
          "data": {
            "access_token": "at",
            "access_expires_in_secs": 3600,
            "refresh_token": "rt",
            "refresh_expires_in_secs": 86400,
            "token_type": "Bearer",
            "user": { "id": "u1", "username": "alice", "role": "admin" }
          }
        }
        """.data(using: .utf8)!

        URLProtocolStub.stubResponse = (200, payload)

        let viewModel = AuthViewModel()
        let authManager = AuthManager()

        let token = try await viewModel.pair(
            serverUrl: "https://srv.example.com/",
            code: "sb_pair_abc",
            authManager: authManager,
            session: session
        )

        XCTAssertEqual(token.accessToken, "at")
        XCTAssertEqual(authManager.serverUrl, "https://srv.example.com")
        XCTAssertEqual(authManager.user?.username, "alice")
    }
}
