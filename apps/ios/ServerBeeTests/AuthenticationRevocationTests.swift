import Foundation
import XCTest
@testable import ServerBee

final class AuthenticationURLProtocol: URLProtocol, @unchecked Sendable {
    nonisolated(unsafe) static var handler: (@Sendable (AuthenticationURLProtocol) -> Void)?

    override static func canInit(with request: URLRequest) -> Bool { true }
    override static func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() { Self.handler?(self) }
    override func stopLoading() {}

    func respond(_ status: Int, data: Data = Data(#"{"data":"ok"}"#.utf8)) {
        guard let url = request.url,
              let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: nil, headerFields: nil) else { return }
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }

    func loseResponse() { client?.urlProtocol(self, didFailWithError: URLError(.networkConnectionLost)) }
}

private final class AuthenticationRequestLog: @unchecked Sendable {
    private let lock = NSLock()
    private var requests: [URLRequest] = []
    private var held: AuthenticationURLProtocol?

    func append(_ request: URLRequest) -> Int {
        lock.lock()
        defer { lock.unlock() }
        requests.append(request)
        return requests.filter { $0.url?.path == request.url?.path }.count
    }

    func hold(_ request: AuthenticationURLProtocol) {
        lock.lock()
        defer { lock.unlock() }
        held = request
    }

    func takeHeld() -> AuthenticationURLProtocol? {
        lock.lock()
        defer { lock.unlock() }
        let request = held
        held = nil
        return request
    }

    func snapshot() -> [URLRequest] {
        lock.lock()
        defer { lock.unlock() }
        return requests
    }
}

private actor RefreshCompletionGate {
    private var continuation: CheckedContinuation<Void, Never>?
    private var released = false

    func wait() async {
        if released { return }
        await withCheckedContinuation { continuation = $0 }
    }

    func open() {
        released = true
        continuation?.resume()
        continuation = nil
    }
}

@MainActor
final class AuthenticationRevocationTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(AuthenticationURLProtocol.self) }

    override func tearDown() async throws {
        URLProtocol.unregisterClass(AuthenticationURLProtocol.self)
        AuthenticationURLProtocol.handler = nil
        AuthManager().clearAuth()
    }

    private func signIn(_ auth: AuthManager, user: String) {
        auth.setServerUrl("https://same-deployment.test")
        auth.handleLoginResponse(MobileTokenResponse(
            accessToken: "access-\(user)", accessExpiresInSecs: 900,
            refreshToken: "refresh-\(user)", refreshExpiresInSecs: 3600,
            tokenType: "Bearer", user: MobileUser(id: user, username: user, role: "member"),
            revocationToken: "revocation-\(user)"
        ))
    }

    private func response(user: String) -> Data {
        Data("""
        {"data":{
          "access_token":"rotated-\(user)","access_expires_in_secs":900,
          "refresh_token":"rotated-refresh-\(user)","refresh_expires_in_secs":3600,
          "token_type":"Bearer","user":{"id":"\(user)","username":"\(user)","role":"member"}
        }}
        """.utf8)
    }

    func testCompletedOldRefreshCannotSupplyReplacementLoginUpload() async {
        let gate = RefreshCompletionGate()
        let completed = expectation(description: "old refresh task completed before coordinator cleanup")
        let coordinator = RefreshCoordinator { result in
            if result.accessToken == "rotated-alice" {
                completed.fulfill()
                await gate.wait()
            }
        }
        let auth = AuthManager(refreshCoordinator: coordinator)
        signIn(auth, user: "alice")
        let aliceResponse = response(user: "alice")
        AuthenticationURLProtocol.handler = { request in request.respond(200, data: aliceResponse) }
        let oldRefresh = Task { try await auth.refreshAccessToken() }
        await fulfillment(of: [completed], timeout: 3)
        XCTAssertEqual(auth.getAccessToken(), "rotated-alice", "A's response was already applied")
        auth.clearAuth()
        signIn(auth, user: "bob")
        let retried = expectation(description: "B upload retried with B credential")
        retried.assertForOverFulfill = false
        let log = AuthenticationRequestLog()
        let bobResponse = response(user: "bob")
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            switch request.request.url?.path {
            case "/api/mobile/auth/refresh": request.respond(200, data: bobResponse)
            case "/api/mobile/push/register":
                if request.request.value(forHTTPHeaderField: "Authorization") == "Bearer access-bob" {
                    request.respond(401)
                } else {
                    XCTAssertEqual(request.request.value(forHTTPHeaderField: "Authorization"), "Bearer rotated-bob")
                    retried.fulfill()
                    request.respond(200)
                }
            default: request.respond(200)
            }
        }
        let manager = PushNotificationManager()
        manager.configure(apiClient: APIClient(authManager: auth))
        manager.didRegisterForRemoteNotifications(deviceToken: Data([1, 2]))
        await fulfillment(of: [retried], timeout: 3)
        XCTAssertEqual(log.snapshot().filter { $0.url?.path == "/api/mobile/auth/refresh" }.count, 1)
        XCTAssertEqual(log.snapshot().filter { $0.url?.path == "/api/mobile/push/register" }.count, 2)
        await gate.open()
        do {
            _ = try await oldRefresh.value
            XCTFail("Old result must be rejected after login replacement")
        } catch AuthError.staleIdentity {
            // Expected: the completed result remains tagged with A's generation.
        } catch { XCTFail("Unexpected old refresh error: \(error)") }
        XCTAssertEqual(auth.user?.id, "bob")
        XCTAssertEqual(auth.getAccessToken(), "rotated-bob")
        await manager.unregister()
    }

    func testLogoutRevokesWhenRefreshSucceedsWithDelayedResponse() async {
        await assertLogoutDuringRefresh(outcome: "success")
    }

    func testLogoutRevokesWhenCommittedRefreshResponseIsLost() async {
        await assertLogoutDuringRefresh(outcome: "lost")
    }

    func testLogoutRevokesWhenCommittedRefreshResponseIsMalformed() async {
        await assertLogoutDuringRefresh(outcome: "malformed")
    }

    func testLegacyFirstRefreshCanLoseResponseWithoutStrandingRegistration() async {
        await assertLogoutDuringRefresh(outcome: "lost", legacy: true)
    }

    func testLegacyFirstRefreshPreservesProofAcrossSuccessfulResponse() async {
        await assertLogoutDuringRefresh(outcome: "success", legacy: true)
    }

    private func assertLogoutDuringRefresh(outcome: String, legacy: Bool = false) async {
        let auth = AuthManager()
        signIn(auth, user: "alice")
        if legacy { KeychainService.delete(for: KeychainService.revocationTokenKey) }
        let api = APIClient(authManager: auth)
        let manager = PushNotificationManager()
        manager.configure(apiClient: api)
        let refreshStarted = expectation(description: "refresh held at HTTP boundary")
        let cleanupStarted = expectation(description: "logout attempts cleanup before refresh resolves")
        let revoked = expectation(description: "stable proof revokes original session")
        revoked.assertForOverFulfill = false
        let log = AuthenticationRequestLog()
        let installationId = InstallationID.getOrCreate()
        let proof = legacy ? "refresh-alice" : "revocation-alice"
        AuthenticationURLProtocol.handler = { request in
            let count = log.append(request.request)
            switch request.request.url?.path {
            case "/api/mobile/auth/refresh":
                if count == 1 {
                    log.hold(request)
                    refreshStarted.fulfill()
                } else { request.respond(401) }
            case "/api/mobile/push/unregister":
                if count == 1 { cleanupStarted.fulfill() }
                request.respond(request.request.value(forHTTPHeaderField: "Authorization") == "Bearer rotated-alice" ? 200 : 401)
            case "/api/mobile/auth/revoke":
                XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
                let body = Self.requestBody(request.request)
                XCTAssertEqual(body["installation_id"] as? String, installationId)
                XCTAssertEqual(body["revocation_token"] as? String, proof)
                revoked.fulfill()
                request.respond(200)
            default:
                XCTFail("Unexpected request: \(request.request.url?.path ?? "nil")")
                request.respond(400)
            }
        }
        let refresh = Task { try await auth.refreshAccessToken() }
        await fulfillment(of: [refreshStarted], timeout: 3)
        let settings = SettingsViewModel()
        let logout = Task {
            await settings.logout(authManager: auth, apiClient: api, pushManager: manager, closeWebSocket: {})
        }
        await fulfillment(of: [cleanupStarted], timeout: 3)
        let pending = log.takeHeld()
        if outcome == "success" { pending?.respond(200, data: response(user: "alice")) }
        if outcome == "lost" { pending?.loseResponse() }
        if outcome == "malformed" { pending?.respond(200, data: Data("invalid response".utf8)) }
        _ = try? await refresh.value
        await logout.value
        await fulfillment(of: [revoked], timeout: 3)
        XCTAssertFalse(auth.isAuthenticated)
        XCTAssertNil(auth.getAccessToken())
        XCTAssertNil(KeychainService.loadString(for: KeychainService.revocationTokenKey))
        XCTAssertEqual(log.snapshot().filter { $0.url?.path == "/api/mobile/auth/revoke" }.count, 1)
        XCTAssertTrue(log.snapshot().allSatisfy { $0.url?.host == "same-deployment.test" })
    }
}

extension AuthenticationRevocationTests {
    func testLegacyOrdinaryRefreshKeepsCapturedRegistrationAndOriginalProof() async throws {
        let auth = AuthManager()
        signIn(auth, user: "alice")
        KeychainService.delete(for: KeychainService.revocationTokenKey)
        let context = try XCTUnwrap(auth.captureContext())
        let payload = response(user: "alice")
        let registered = expectation(description: "captured registration uses current rotated credential")
        AuthenticationURLProtocol.handler = { request in
            if request.request.url?.path == "/api/mobile/auth/refresh" {
                request.respond(200, data: payload)
            } else {
                XCTAssertEqual(request.request.url?.path, "/api/mobile/push/register")
                XCTAssertEqual(request.request.value(forHTTPHeaderField: "Authorization"), "Bearer rotated-alice")
                registered.fulfill()
                request.respond(200)
            }
        }
        for _ in 0..<2 {
            let token = try await auth.refreshAccessToken(context: context)
            XCTAssertEqual(token, "rotated-alice")
            XCTAssertTrue(auth.isCurrent(context))
            XCTAssertEqual(KeychainService.loadString(for: KeychainService.revocationTokenKey), "refresh-alice")
        }
        try await APIClient(authManager: auth).postVoid(
            "/api/mobile/push/register", body: ["device_token": "still-registered"], context: context
        )
        await fulfillment(of: [registered], timeout: 3)
    }

    func testRestoredLegacyFirstRefreshResponseLossRevokesBeforeClearingProof() async {
        let original = AuthManager()
        signIn(original, user: "alice")
        KeychainService.delete(for: KeychainService.revocationTokenKey)
        let log = AuthenticationRequestLog()
        let revoked = expectation(description: "restored legacy proof revokes after response loss")
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            switch request.request.url?.path {
            case "/api/mobile/auth/refresh": request.loseResponse()
            case "/api/mobile/auth/revoke":
                XCTAssertEqual(Self.requestBody(request.request)["revocation_token"] as? String, "refresh-alice")
                XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
                revoked.fulfill()
                request.respond(200)
            default:
                XCTFail("Restored cleanup must use the captured deletion-only proof")
                request.respond(401)
            }
        }
        let restored = AuthManager()
        await restored.initialize()
        await fulfillment(of: [revoked], timeout: 3)
        XCTAssertFalse(restored.isAuthenticated)
        XCTAssertNil(restored.getAccessToken())
        XCTAssertNil(KeychainService.loadString(for: KeychainService.revocationTokenKey))
        XCTAssertEqual(log.snapshot().compactMap { $0.url?.path }, [
            "/api/mobile/auth/refresh", "/api/mobile/auth/revoke"
        ])
    }
}

private extension AuthenticationRevocationTests {
    nonisolated static func requestBody(_ request: URLRequest) -> [String: Any] {
        var data = request.httpBody ?? Data()
        if data.isEmpty, let stream = request.httpBodyStream {
            stream.open()
            defer { stream.close() }
            var buffer = [UInt8](repeating: 0, count: 1024)
            while stream.hasBytesAvailable {
                let count = stream.read(&buffer, maxLength: buffer.count)
                if count <= 0 { break }
                data.append(contentsOf: buffer.prefix(count))
            }
        }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
    }
}
