import XCTest
@testable import ServerBee

@MainActor
final class AuthViewModelTests: XCTestCase {
    func testLoginShowsErrorOnNonHTTPResponse() async {
        URLProtocol.registerClass(URLProtocolStub.self)
        defer { URLProtocol.unregisterClass(URLProtocolStub.self) }
        // Simulate a transport error so URLSession produces no HTTPURLResponse.
        URLProtocolStub.stubResponseFactory = nil
        URLProtocolStub.stubResponse = nil
        URLProtocolStub.stubError = URLError(.cannotConnectToHost)

        let auth = AuthManager()
        let vm = AuthViewModel()
        vm.serverUrlInput = "https://stub.test"
        vm.username = "u"
        vm.password = "p"

        await vm.login(authManager: auth)
        XCTAssertFalse(vm.errorMessage.isEmpty, "Expected a user-facing error rather than a crash")
        XCTAssertFalse(vm.isLoading)
    }

    func testLoginHandlesNonHTTPURLResponse() async {
        URLProtocol.registerClass(NonHTTPURLProtocolStub.self)
        defer { URLProtocol.unregisterClass(NonHTTPURLProtocolStub.self) }

        let auth = AuthManager()
        let vm = AuthViewModel()
        vm.serverUrlInput = "https://stub.test"
        vm.username = "u"
        vm.password = "p"

        await vm.login(authManager: auth)
        XCTAssertFalse(vm.errorMessage.isEmpty)
    }
}

/// Returns a bare `URLResponse` (not `HTTPURLResponse`) to exercise the
/// non-HTTP branch.
final class NonHTTPURLProtocolStub: URLProtocol {
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        let response = URLResponse(url: request.url!, mimeType: "text/plain",
                                   expectedContentLength: 0, textEncodingName: nil)
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data())
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}
