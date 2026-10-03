import Foundation
import XCTest
@testable import ServerBee

@MainActor
final class AutomaticAuthenticationExpiryTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(AuthenticationURLProtocol.self) }

    override func tearDown() async throws {
        URLProtocol.unregisterClass(AuthenticationURLProtocol.self)
        AuthenticationURLProtocol.handler = nil
        AuthManager().clearAuth()
        try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
    }

    private func signIn(_ auth: AuthManager, user: String = "alice", legacy: Bool = false,
                        server: String = "https://original-deployment.test") {
        auth.setServerUrl(server)
        auth.handleLoginResponse(MobileTokenResponse(
            accessToken: "access-\(user)", accessExpiresInSecs: 900,
            refreshToken: "refresh-\(user)", refreshExpiresInSecs: 3600,
            tokenType: "Bearer", user: MobileUser(id: user, username: user, role: "member"),
            revocationToken: legacy ? nil : "revoke-\(user)",
            mobileSessionId: legacy ? nil : (user == "alice" ? "11111111-1111-4111-8111-111111111111" : "22222222-2222-4222-8222-222222222222")
        ))
    }

    private func ordinaryRequest(_ api: APIClient, kind: String, context: MobileAuthenticationContext) async throws {
        switch kind {
        case "GET": let _: String = try await api.get("/api/servers")
        case "POST": let _: String = try await api.post("/api/servers")
        case "PUT": let _: String = try await api.put("/api/servers")
        case "DELETE": let _: String = try await api.delete("/api/servers")
        case "POST-VOID": try await api.postVoid("/api/servers")
        case "DELETE-VOID": try await api.deleteVoid("/api/servers")
        default:
            try await api.postVoid("/api/mobile/push/register", body: ["device_token": "apns"], context: context)
        }
    }

    func testEveryAPIEntryRevokesBeforeAutomaticLogoutAfterLostRefresh() async throws {
        for kind in ["GET", "POST", "PUT", "DELETE", "POST-VOID", "DELETE-VOID", "CAPTURED-POST"] {
            try await assertAutomaticLogout(kind: kind, legacy: false, loseFirstRefresh: true)
        }
    }

    func testLegacyAPIEntryRevokesWithProofSavedBeforeFirstLostRefresh() async throws {
        try await assertAutomaticLogout(kind: "GET", legacy: true, loseFirstRefresh: true)
        try await assertAutomaticLogout(kind: "CAPTURED-POST", legacy: true, loseFirstRefresh: true)
    }

    func testEveryAPIRetry401RevokesTheOriginalLogin() async throws {
        for kind in ["GET", "POST", "PUT", "DELETE", "POST-VOID", "DELETE-VOID", "CAPTURED-POST"] {
            try await assertAutomaticLogout(kind: kind, legacy: false, loseFirstRefresh: false)
        }
    }

    private func assertAutomaticLogout(kind: String, legacy: Bool, loseFirstRefresh: Bool) async throws {
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [AuthenticationURLProtocol.self]))
        auth.clearAuth()
        signIn(auth, legacy: legacy)
        let context = try XCTUnwrap(auth.captureContext())
        let api = APIClient(authManager: auth)
        let log = AuthenticationRequestLog()
        let proof = legacy ? "refresh-alice" : "revoke-alice"
        let rotated = Data("""
        {"data":{"access_token":"rotated-alice","access_expires_in_secs":900,
        "refresh_token":"rotated-refresh","refresh_expires_in_secs":3600,"token_type":"Bearer",
        "user":{"id":"alice","username":"alice","role":"member"}}}
        """.utf8)
        AuthenticationURLProtocol.handler = { request in
            let count = log.append(request.request)
            switch request.request.url?.path {
            case "/api/mobile/auth/refresh":
                if !loseFirstRefresh { request.respond(200, data: rotated) } else if count == 1 {
                    request.loseResponse()
                } else { request.respond(401) }
            case "/api/mobile/auth/revoke":
                XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
                let payload = Self.body(request.request)
                XCTAssertEqual(payload["revocation_token"] as? String, proof)
                XCTAssertEqual(payload["installation_id"] as? String, context.installationId)
                XCTAssertEqual(payload["expected_session_id"] as? String, legacy ? nil : "11111111-1111-4111-8111-111111111111")
                request.respond(200)
            default: request.respond(401)
            }
        }
        if loseFirstRefresh {
            do {
                _ = try await auth.refreshAccessToken()
                XCTFail("Committed rotation's response must be lost at the network boundary")
            } catch AuthError.refreshNetworkFailure { /* expected transport failure */ }
            XCTAssertTrue(auth.isAuthenticated, "The first transport failure must preserve the captured login")
            XCTAssertEqual((try? AuthManager.readAuthentication())?.revocationToken, proof)
        }
        do {
            try await ordinaryRequest(api, kind: kind, context: context)
            XCTFail("Rejected credentials must surface unauthorized")
        } catch APIError.unauthorized { /* expected automatic logout */ }
        let requests = log.snapshot()
        XCTAssertEqual(requests.filter { $0.url?.path == "/api/mobile/auth/revoke" }.count, 1)
        XCTAssertEqual(requests.last?.url?.path, "/api/mobile/auth/revoke", "Cleanup targets the original login without reauthentication")
        XCTAssertTrue(requests.allSatisfy { $0.url?.host == "original-deployment.test" })
        if !loseFirstRefresh {
            let retries = requests.filter { $0.url?.path != "/api/mobile/auth/refresh" && $0.url?.path != "/api/mobile/auth/revoke" }
            XCTAssertEqual(retries.count, 2)
            XCTAssertEqual(retries.last?.value(forHTTPHeaderField: "Authorization"), "Bearer rotated-alice")
        }
        XCTAssertFalse(auth.isAuthenticated)
        XCTAssertNil(auth.getAccessToken())
        XCTAssertNil(try AuthManager.readAuthentication())
    }

    func testAutomaticExpiryCleanupCannotClearOrRevokeReplacementLogin() async throws {
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [AuthenticationURLProtocol.self]))
        signIn(auth)
        let log = AuthenticationRequestLog()
        let cleanupStarted = expectation(description: "original revocation is in flight")
        cleanupStarted.assertForOverFulfill = false
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            if request.request.url?.path == "/api/mobile/auth/revoke" {
                XCTAssertEqual(Self.body(request.request)["revocation_token"] as? String, "revoke-alice")
                XCTAssertNil(request.request.value(forHTTPHeaderField: "Authorization"))
                log.hold(request)
                cleanupStarted.fulfill()
            } else { request.respond(401) }
        }
        let api = APIClient(authManager: auth)
        let expired = Task { let _: String = try await api.get("/api/servers") }
        await fulfillment(of: [cleanupStarted], timeout: 3)
        XCTAssertFalse(auth.isAuthenticated, "Durable proof permits local logout before the Server replies")
        XCTAssertNil(try AuthManager.readAuthentication())
        let pending = try PendingSessionRevocations().records()
        XCTAssertEqual(pending.map(\.proof), ["revoke-alice"])
        XCTAssertEqual(pending.map(\.mobileSessionId), ["11111111-1111-4111-8111-111111111111"])
        signIn(auth, user: "bob", server: "https://replacement-deployment.test")
        try XCTUnwrap(log.takeHeld()).respond(200)
        do {
            try await expired.value
            XCTFail("Original request must remain unauthorized")
        } catch APIError.unauthorized { /* expected */ }
        XCTAssertEqual(auth.user?.id, "bob")
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertEqual(auth.getAccessToken(), "access-bob")
        XCTAssertEqual((try? AuthManager.readAuthentication())?.revocationToken, "revoke-bob")
        XCTAssertTrue(log.snapshot().allSatisfy { $0.url?.host == "original-deployment.test" })
        XCTAssertEqual(log.snapshot().filter { $0.url?.path == "/api/mobile/auth/revoke" }.count, 1)
    }

    func testLateOld401CannotRefreshOrClearReplacementLogin() async throws {
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [AuthenticationURLProtocol.self]))
        signIn(auth)
        let log = AuthenticationRequestLog()
        let started = expectation(description: "old ordinary request in flight")
        started.assertForOverFulfill = false
        AuthenticationURLProtocol.handler = { request in
            _ = log.append(request.request)
            log.hold(request)
            started.fulfill()
        }
        let api = APIClient(authManager: auth)
        let oldRequest = Task { let _: String = try await api.get("/api/servers") }
        await fulfillment(of: [started], timeout: 3)
        signIn(auth, user: "bob")
        try XCTUnwrap(log.takeHeld()).respond(401)
        do {
            try await oldRequest.value
            XCTFail("Old response must reject its stale identity")
        } catch AuthError.staleIdentity { /* expected */ }
        XCTAssertEqual(auth.user?.id, "bob")
        XCTAssertEqual(auth.getAccessToken(), "access-bob")
        XCTAssertEqual(log.snapshot().map { $0.url?.path }, ["/api/servers"])
    }

    func testReconnectExpiryRevokesAfterCommittedResponseLoss() async throws {
        for legacy in [false, true] {
            let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [AuthenticationURLProtocol.self]))
            signIn(auth, legacy: legacy)
            let log = AuthenticationRequestLog()
            AuthenticationURLProtocol.handler = { request in
                let count = log.append(request.request)
                if request.request.url?.path == "/api/mobile/auth/refresh" {
                    if count == 1 { request.loseResponse() } else { request.respond(401) }
                } else {
                    XCTAssertEqual(request.request.url?.path, "/api/mobile/auth/revoke")
                    XCTAssertEqual(Self.body(request.request)["revocation_token"] as? String, legacy ? "refresh-alice" : "revoke-alice")
                    request.respond(200)
                }
            }
            do { _ = try await auth.refreshAccessToken(); XCTFail("Expected lost response") } catch AuthError.refreshNetworkFailure { /* expected */ }
            let token = await auth.accessTokenForReconnect()
            XCTAssertNil(token)
            XCTAssertFalse(auth.isAuthenticated)
            XCTAssertNil(try AuthManager.readAuthentication())
            XCTAssertEqual(log.snapshot().map { $0.url?.path }, [
                "/api/mobile/auth/refresh", "/api/mobile/auth/refresh", "/api/mobile/auth/revoke"
            ])
        }
    }

    func testRejectedOldServerLogoutFallbackCannotReenterExpiryOrDiscardCredentials() async throws {
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [AuthenticationURLProtocol.self]))
        signIn(auth, legacy: true)
        let log = AuthenticationRequestLog()
        AuthenticationURLProtocol.handler = { request in
            let count = log.append(request.request)
            if count > 1 {
                XCTFail("Expiry cleanup must not recursively repeat requests")
                request.respond(200)
            } else if request.request.url?.path == "/api/mobile/auth/revoke" {
                request.respond(404)
            } else { request.respond(401) }
        }
        do {
            let _: String = try await APIClient(authManager: auth).get("/api/servers")
            XCTFail("Expired legacy Server credentials must remain unauthorized")
        } catch APIError.unauthorized { /* expected */ }
        XCTAssertTrue(auth.isAuthenticated, "Unknown legacy material must be retained until reachable cleanup succeeds")
        XCTAssertEqual((try AuthManager.readAuthentication())?.refreshToken, "refresh-alice")
        XCTAssertNotNil(auth.recoveryError)
        XCTAssertTrue(try PendingSessionRevocations().records().isEmpty)
        XCTAssertEqual(log.snapshot().map { $0.url?.path }, [
            "/api/servers", "/api/mobile/auth/refresh", "/api/mobile/auth/revoke", "/api/mobile/auth/logout"
        ])
        XCTAssertEqual(log.snapshot().last?.value(forHTTPHeaderField: "Authorization"), "Bearer access-alice")
    }

    nonisolated private static func body(_ request: URLRequest) -> [String: Any] {
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
