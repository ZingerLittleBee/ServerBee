import XCTest
@testable import ServerBee

@MainActor
final class AuthViewModelTests: XCTestCase {
    override func setUp() async throws {
        AuthManager().clearAuth()
        try KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
        URLProtocol.registerClass(AuthenticationURLProtocol.self)
    }

    override func tearDown() async throws {
        AuthManager().clearAuth()
        try KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
        URLProtocolStub.stubError = nil
        AuthenticationURLProtocol.handler = nil
        AuthenticationURLProtocol.cancelPending()
        URLProtocol.unregisterClass(AuthenticationURLProtocol.self)
    }

    func testLoginShowsErrorOnNonHTTPResponse() async {
        URLProtocol.registerClass(URLProtocolStub.self)
        defer { URLProtocol.unregisterClass(URLProtocolStub.self) }
        // Simulate a transport error so URLSession produces no HTTPURLResponse.
        URLProtocolStub.stubResponseFactory = nil
        URLProtocolStub.stubResponse = nil
        URLProtocolStub.stubError = URLError(.cannotConnectToHost)

        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [URLProtocolStub.self]))
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

        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [NonHTTPURLProtocolStub.self]))
        let vm = AuthViewModel()
        vm.serverUrlInput = "https://stub.test"
        vm.username = "u"
        vm.password = "p"

        await vm.login(authManager: auth)
        XCTAssertFalse(vm.errorMessage.isEmpty)
    }
    func testLoginAndPairCleanTheCurrentIdentityAfterDrainingOlderRecords() async throws {
        for route in ["login", "pair"] {
            AuthManager().clearAuth()
            let journal = PreparationRevocationStorage()
            let auth = preparationAuth(journal)
            let original = try XCTUnwrap(AuthManager.readAuthentication()?.revocation)
            let older = PendingSessionRevocation(id: UUID(), serverUrl: "https://older.test", userId: "older",
                installationId: original.installationId, mobileSessionId: UUID().uuidString, proof: "older-deletion")
            journal.values = [older]
            let started = expectation(description: "older replay held before " + route)
            let log = AuthenticationRequestLog()
            let replacement = try preparationResponse("bob")
            AuthenticationURLProtocol.handler = { request in
                _ = log.append(request.request)
                if request.request.url?.path == "/api/mobile/auth/revoke" {
                    let body = PushSetupTestData.body(request.request)
                    XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
                    if body["expected_session_id"] as? String == older.mobileSessionId {
                        log.hold(request)
                        started.fulfill()
                    } else {
                        XCTAssertEqual(body["expected_session_id"] as? String, original.mobileSessionId)
                        XCTAssertEqual(body["revocation_token"] as? String, original.proof)
                        XCTAssertEqual(request.request.url?.host, "current.test")
                        request.respond(200)
                    }
                } else {
                    XCTAssertEqual(request.request.url?.path, "/api/mobile/auth/" + route)
                    XCTAssertEqual(log.snapshot().count, 3, "Current-session cleanup must precede replacement HTTP")
                    request.respond(200, data: replacement)
                }
            }
            let viewModel = preparationViewModel()
            let attempt = Task {
                if route == "login" { await viewModel.login(authManager: auth) }
                else {
                    do { _ = try await viewModel.pair(serverUrl: "https://replacement.test", code: "fixture", authManager: auth) }
                    catch { XCTFail("Unexpected pairing error: \(error)") }
                }
            }
            await fulfillment(of: [started], timeout: 3)
            XCTAssertEqual(log.snapshot().count, 1)
            try XCTUnwrap(log.takeHeld()).respond(200, data: Data(#"{"data":"already_absent"}"#.utf8))
            await attempt.value
            XCTAssertEqual(auth.user?.id, "bob")
            XCTAssertEqual(auth.serverUrl, "https://replacement.test")
            XCTAssertEqual(log.snapshot().compactMap { $0.url?.path }, [
                "/api/mobile/auth/revoke", "/api/mobile/auth/revoke", "/api/mobile/auth/" + route
            ])
        }
    }

    func testNewerLoginDuringPreparationPreventsBothStaleLoginAndPairRequests() async throws {
        for route in ["login", "pair"] {
            for heldStep in ["older", "current"] {
                AuthManager().clearAuth()
                let journal = PreparationRevocationStorage()
                let auth = preparationAuth(journal)
                let original = try XCTUnwrap(AuthManager.readAuthentication()?.revocation)
                let older = PendingSessionRevocation(id: UUID(), serverUrl: "https://older.test", userId: "older",
                    installationId: original.installationId, mobileSessionId: UUID().uuidString, proof: "older-deletion")
                if heldStep == "older" { journal.values = [older] }
                let started = expectation(description: route + " held during " + heldStep + " cleanup")
                let log = AuthenticationRequestLog()
                let expected = heldStep == "older" ? older : original
                AuthenticationURLProtocol.handler = { request in
                    _ = log.append(request.request)
                    XCTAssertEqual(request.request.url?.path, "/api/mobile/auth/revoke", "Stale preparation cannot send replacement HTTP")
                    let body = PushSetupTestData.body(request.request)
                    XCTAssertEqual(body["expected_session_id"] as? String, expected.mobileSessionId)
                    XCTAssertEqual(body["revocation_token"] as? String, expected.proof, "Never adopt the newer login's proof")
                    if log.snapshot().count == 1 { log.hold(request); started.fulfill() }
                    else { XCTFail("The older attempt must stop after identity replacement"); request.respond(500) }
                }
                let viewModel = preparationViewModel()
                let attempt = Task {
                    if route == "login" { await viewModel.login(authManager: auth) }
                    else {
                        do {
                            _ = try await viewModel.pair(serverUrl: "https://replacement.test", code: "fixture", authManager: auth)
                            XCTFail("Expected stale pairing attempt")
                        } catch AuthError.staleIdentity { } catch { XCTFail("Unexpected pairing error: \(error)") }
                    }
                }
                await fulfillment(of: [started], timeout: 3)
                auth.handleLoginResponse(preparationTokens("bob"), origin: "https://newer.test", installationId: original.installationId)
                let replacementGeneration = auth.authenticationGeneration
                XCTAssertEqual(auth.user?.id, "bob")
                try XCTUnwrap(log.takeHeld()).respond(200)
                await attempt.value
                XCTAssertEqual(log.snapshot().count, 1)
                XCTAssertEqual(auth.authenticationGeneration, replacementGeneration)
                XCTAssertEqual(auth.user?.id, "bob")
                XCTAssertEqual(auth.serverUrl, "https://newer.test")
                XCTAssertEqual(auth.getAccessToken(), "access-bob")
                XCTAssertFalse(journal.values.contains { $0.userId == "bob" })
                if route == "login" { XCTAssertFalse(viewModel.errorMessage.isEmpty) }
            }
        }
    }

    func testPreparationRetainsCurrentRecordWhenItsDirectCleanupFails() async throws {
        let journal = PreparationRevocationStorage()
        let auth = preparationAuth(journal)
        let original = try XCTUnwrap(AuthManager.readAuthentication()?.revocation)
        let log = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            XCTAssertEqual(PushSetupTestData.body(request.request)["expected_session_id"] as? String, original.mobileSessionId)
            request.respond(503)
        }
        try await auth.prepareForLogin()
        XCTAssertEqual(log.snapshot().count, 1)
        XCTAssertEqual(journal.values, [original])
        XCTAssertEqual(auth.user?.id, "alice")
    }

    private func preparationAuth(_ journal: PreparationRevocationStorage) -> AuthManager {
        let auth = AuthManager(revocations: PendingSessionRevocations(storage: journal),
                               cleanupSession: APIClient.makeCleanupSession(protocolClasses: [AuthenticationURLProtocol.self]))
        auth.handleLoginResponse(preparationTokens("alice"), origin: "https://current.test")
        return auth
    }

    private func preparationViewModel() -> AuthViewModel {
        let viewModel = AuthViewModel()
        viewModel.serverUrlInput = "https://replacement.test"
        viewModel.username = "bob"
        viewModel.password = "fixture-password"
        return viewModel
    }

    private func preparationTokens(_ name: String) -> MobileTokenResponse {
        MobileTokenResponse(accessToken: "access-\(name)", accessExpiresInSecs: 900,
            refreshToken: "refresh-\(name)", refreshExpiresInSecs: 3600, tokenType: "Bearer",
            user: MobileUser(id: name, username: name, role: "member"), revocationToken: "deletion-\(name)",
            mobileSessionId: name == "alice" ? "11111111-1111-4111-8111-111111111111" : "22222222-2222-4222-8222-222222222222")
    }

    private func preparationResponse(_ name: String) throws -> Data {
        var data = Data(#"{"data":"#.utf8)
        data.append(try JSONEncoder().encode(preparationTokens(name)))
        data.append(Data("}".utf8))
        return data
    }
}

@MainActor
private final class PreparationRevocationStorage: SessionRevocationStorage {
    var values: [PendingSessionRevocation] = []
    func read() throws -> [PendingSessionRevocation] { values }
    func write(_ records: [PendingSessionRevocation]) throws { values = records }
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
