import XCTest
@testable import ServerBee

/// Stubs `URLSession.shared.data(for:)` by intercepting via `URLProtocol`.
final class URLProtocolStub: URLProtocol {
    nonisolated(unsafe) static var stubResponse: (status: Int, data: Data)?
    nonisolated(unsafe) static var stubError: Error?
    nonisolated(unsafe) static var stubResponseFactory: (@Sendable () async -> (status: Int, data: Data))?

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        if let error = Self.stubError {
            client?.urlProtocol(self, didFailWithError: error)
            return
        }
        if let factory = Self.stubResponseFactory {
            let url = request.url!
            let semaphore = DispatchSemaphore(value: 0)
            nonisolated(unsafe) var resolved: (status: Int, data: Data)?
            Task {
                resolved = await factory()
                semaphore.signal()
            }
            semaphore.wait()
            guard let (status, data) = resolved else { return }
            emit(url: url, status: status, data: data)
            return
        }
        guard let (status, data) = Self.stubResponse else { return }
        emit(url: request.url!, status: status, data: data)
    }

    private func emit(url: URL, status: Int, data: Data) {
        let response = HTTPURLResponse(
            url: url,
            statusCode: status,
            httpVersion: "HTTP/1.1",
            headerFields: nil
        )!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}

@MainActor
final class RefreshErrorClassificationTests: XCTestCase {
    override func setUp() async throws {
        URLProtocol.registerClass(URLProtocolStub.self)
        URLProtocolStub.stubResponse = nil
        URLProtocolStub.stubError = nil
        URLProtocolStub.stubResponseFactory = nil
    }

    override func tearDown() async throws {
        URLProtocol.unregisterClass(URLProtocolStub.self)
        URLProtocolStub.stubResponse = nil
        URLProtocolStub.stubError = nil
        URLProtocolStub.stubResponseFactory = nil
    }

    func test401MapsToRefreshUnauthorized() async {
        URLProtocolStub.stubResponse = (401, Data())
        let auth = AuthManager()
        auth.serverUrl = "https://stub.test"
        try? KeychainService.saveString("rt", for: KeychainService.refreshTokenKey)

        do {
            _ = try await auth.refreshAccessToken()
            XCTFail("Expected throw")
        } catch let err as AuthError {
            if case .refreshUnauthorized = err { /* ok */ } else {
                XCTFail("Expected refreshUnauthorized, got \(err)")
            }
        } catch {
            XCTFail("Unexpected error: \(error)")
        }
    }

    func test503MapsToRefreshNetworkFailure() async {
        URLProtocolStub.stubResponse = (503, Data())
        let auth = AuthManager()
        auth.serverUrl = "https://stub.test"
        try? KeychainService.saveString("rt", for: KeychainService.refreshTokenKey)

        do {
            _ = try await auth.refreshAccessToken()
            XCTFail("Expected throw")
        } catch let err as AuthError {
            if case .refreshNetworkFailure = err { /* ok */ } else {
                XCTFail("Expected refreshNetworkFailure, got \(err)")
            }
        } catch {
            XCTFail("Unexpected error: \(error)")
        }
    }

    func testAPIClientClearsAuthOnly_OnUnauthorized() async throws {
        URLProtocolStub.stubResponse = (401, Data())
        let auth = AuthManager()
        auth.serverUrl = "https://stub.test"
        auth.isAuthenticated = true
        try? KeychainService.saveString("rt", for: KeychainService.refreshTokenKey)

        let client = APIClient(authManager: auth)
        do {
            let _: ApiResponse<String> = try await client.get("/anything")
            XCTFail("Expected throw")
        } catch {
            // 401 path
        }
        XCTAssertFalse(auth.isAuthenticated, "401 from refresh must clear auth")
    }

    func testAPIClientPreservesAuth_OnNetworkFailure() async throws {
        URLProtocolStub.stubResponse = (503, Data())
        let auth = AuthManager()
        auth.serverUrl = "https://stub.test"
        auth.isAuthenticated = true
        try? KeychainService.saveString("rt", for: KeychainService.refreshTokenKey)

        let client = APIClient(authManager: auth)
        do {
            let _: ApiResponse<String> = try await client.get("/anything")
            XCTFail("Expected throw")
        } catch {
            // network path
        }
        XCTAssertTrue(auth.isAuthenticated, "Transient network error must NOT clear auth")
    }
}
