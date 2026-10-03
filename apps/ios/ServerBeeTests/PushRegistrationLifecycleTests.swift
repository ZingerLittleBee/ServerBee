import Foundation
import XCTest
@testable import ServerBee

/// Holds URLSession requests at the external HTTP boundary.
final class PushLifecycleURLProtocol: URLProtocol {
    nonisolated(unsafe) static var handler: (@Sendable (PushLifecycleURLProtocol) -> Void)?

    override static func canInit(with request: URLRequest) -> Bool { true }
    override static func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    private static let pending = PendingURLProtocolRequests()
    override func startLoading() {
        Self.pending.begin(self)
        if request.url?.path == "/api/mobile/push/settings", request.httpMethod == "GET" {
            respond(200, data: PushSetupTestData.response())
            return
        }
        if let handler = Self.handler { handler(self) } else {
            if Self.pending.finish(self) { client?.urlProtocol(self, didFailWithError: URLError(.cancelled)) }
        }
    }
    override func stopLoading() { _ = Self.pending.finish(self) }
    static func cancelPending() { pending.cancelAll() }

    func respond(_ status: Int, data: Data = Data(#"{"data":"ok"}"#.utf8)) {
        guard Self.pending.finish(self), let url = request.url,
              let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: nil, headerFields: nil) else { return }
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        let result = request.url?.path == "/api/mobile/push/verified-register" && status == 200 && data == Data(#"{"data":"ok"}"#.utf8)
            ? PushSetupTestData.response(registered: true, revision: 2) : data
        client?.urlProtocol(self, didLoad: result)
        client?.urlProtocolDidFinishLoading(self)
    }
}

private final class HeldPushRequest: @unchecked Sendable {
    private let lock = NSLock()
    private var storage: PushLifecycleURLProtocol?
    private var hasHeld = false

    func holdFirst(_ request: PushLifecycleURLProtocol) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard !hasHeld else { return false }
        hasHeld = true
        storage = request
        return true
    }

    func hold(_ request: PushLifecycleURLProtocol) {
        lock.lock()
        defer { lock.unlock() }
        storage = request
    }

    func release(_ status: Int, data: Data = Data(#"{"data":"ok"}"#.utf8)) {
        lock.lock()
        let request = storage
        storage = nil
        lock.unlock()
        request?.respond(status, data: data)
    }
}

@MainActor
final class PushRegistrationLifecycleTests: XCTestCase {
    override func setUp() async throws {
        URLProtocol.registerClass(PushLifecycleURLProtocol.self)
    }

    override func tearDown() async throws {
        PushLifecycleURLProtocol.handler = nil
        PushLifecycleURLProtocol.cancelPending()
        URLProtocol.unregisterClass(PushLifecycleURLProtocol.self)
        AuthManager().clearAuth()
        try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
    }

    private func signIn(
        _ auth: AuthManager, server: String = "https://original.test", user: String = "alice", accessToken: String? = nil
    ) {
        auth.setServerUrl(server)
        auth.handleLoginResponse(MobileTokenResponse(
            accessToken: accessToken ?? "access-\(user)", accessExpiresInSecs: 900,
            refreshToken: "refresh-\(user)", refreshExpiresInSecs: 3600,
            tokenType: "Bearer", user: MobileUser(id: user, username: user, role: "member"),
            revocationToken: "proof-\(accessToken ?? user)",
            mobileSessionId: user == "alice" && accessToken == nil ? "11111111-1111-4111-8111-111111111111" : "22222222-2222-4222-8222-222222222222"
        ))
    }

    func testStaleUploadDoesNotRetryAgainstReplacementAccountOrDeployment() async {
        await assertStaleUpload(server: "https://replacement.test", user: "bob", status: 401)
    }

    func testSuccessfulStaleUploadCannotUnregisterReplacementAccount() async {
        await assertStaleUpload(server: "https://replacement.test", user: "bob", status: 200)
    }

    func testReloginWithSameAccountInvalidatesOldUpload() async {
        await assertStaleUpload(server: "https://original.test", user: "alice", status: 401)
    }

    private func assertStaleUpload(server: String, user: String, status: Int) async {
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PushLifecycleURLProtocol.self]))
        signIn(auth)
        let manager = PushNotificationManager(system: TestPushSystem(), relay: TestPushRelay(), storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        let started = expectation(description: "original upload started")
        let replacementRequest = expectation(description: "no replacement request")
        replacementRequest.isInverted = true
        replacementRequest.assertForOverFulfill = false
        let held = HeldPushRequest()
        PushLifecycleURLProtocol.handler = { request in
            if request.request.url?.host == "original.test",
               request.request.url?.path == "/api/mobile/push/verified-register", held.holdFirst(request) {
                XCTAssertEqual(request.request.value(forHTTPHeaderField: "Authorization"), "Bearer access-alice")
                started.fulfill()
            } else if request.request.url?.host == "original.test",
                      request.request.url?.path == "/api/mobile/push/unregister",
                      request.request.value(forHTTPHeaderField: "Authorization") == "Bearer access-alice" {
                request.respond(200)
            } else {
                replacementRequest.fulfill()
                request.respond(401)
            }
        }
        manager.didRegisterForRemoteNotifications(deviceToken: Data([1, 2]))
        await fulfillment(of: [started], timeout: 3)
        auth.clearAuth()
        signIn(auth, server: server, user: user, accessToken: "replacement-access-\(user)")
        held.release(status)
        await manager.unregister()
        await fulfillment(of: [replacementRequest], timeout: 0.3)
        XCTAssertEqual(auth.user?.id, user)
        XCTAssertTrue(auth.isAuthenticated)
    }
}

private final class PushRequestLog: @unchecked Sendable {
    private let lock = NSLock()
    private var requests: [URLRequest] = []

    func append(_ request: URLRequest) {
        lock.lock()
        defer { lock.unlock() }
        requests.append(request)
    }

    func snapshot() -> [URLRequest] {
        lock.lock()
        defer { lock.unlock() }
        return requests
    }
}

private extension PushRegistrationLifecycleTests {
    nonisolated static func assertAliceRevocation(_ request: URLRequest) {
        XCTAssertNil(request.value(forHTTPHeaderField: "Authorization"))
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
        let body = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        XCTAssertEqual(body?["revocation_token"] as? String, "proof-alice")
        XCTAssertEqual(body?["expected_session_id"] as? String, "11111111-1111-4111-8111-111111111111")
        XCTAssertNotNil(body?["installation_id"] as? String)
        XCTAssertEqual(body?.count, 3, "Durable cleanup carries only its exact deletion capability")
    }

    func refreshResponse() -> Data {
        Data(#"""
        {"data":{
            "access_token":"rotated-alice","access_expires_in_secs":900,
            "refresh_token":"rotated-refresh","refresh_expires_in_secs":3600,
            "token_type":"Bearer","user":{"id":"alice","username":"alice","role":"member"}
        }}
        """#.utf8)
    }
}

extension PushRegistrationLifecycleTests {
    func testUploadRetriesRefreshOnlyWithinCapturedLogin() async {
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PushLifecycleURLProtocol.self]))
        signIn(auth)
        let manager = PushNotificationManager(system: TestPushSystem(), relay: TestPushRelay(), storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        let retried = expectation(description: "upload retried with rotated credential")
        let log = PushRequestLog()
        let response = refreshResponse()
        PushLifecycleURLProtocol.handler = { request in
            log.append(request.request)
            XCTAssertEqual(request.request.url?.host, "original.test")
            switch request.request.url?.path {
            case "/api/mobile/push/verified-register":
                if request.request.value(forHTTPHeaderField: "Authorization") == "Bearer access-alice" {
                    request.respond(401)
                } else {
                    XCTAssertEqual(request.request.value(forHTTPHeaderField: "Authorization"), "Bearer rotated-alice")
                    retried.fulfill()
                    request.respond(200)
                }
            case "/api/mobile/auth/refresh":
                request.respond(200, data: response)
            case "/api/mobile/push/unregister":
                XCTAssertEqual(request.request.value(forHTTPHeaderField: "Authorization"), "Bearer rotated-alice")
                request.respond(200)
            default:
                XCTFail("Unexpected request")
                request.respond(400)
            }
        }
        manager.didRegisterForRemoteNotifications(deviceToken: Data([1, 2]))
        await fulfillment(of: [retried], timeout: 3)
        XCTAssertEqual(auth.user?.id, "alice")
        XCTAssertEqual(auth.getAccessToken(), "rotated-alice")
        await manager.unregister()
        XCTAssertEqual(log.snapshot().compactMap { $0.url?.path }, [
            "/api/mobile/push/verified-register", "/api/mobile/auth/refresh",
            "/api/mobile/push/verified-register", "/api/mobile/push/unregister"
        ])
    }

    func testRefreshCompletionAfterLogoutCannotRestorePreviousAccount() async {
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PushLifecycleURLProtocol.self]))
        signIn(auth)
        let manager = PushNotificationManager(system: TestPushSystem(), relay: TestPushRelay(), storage: MemoryPushSetupStorage())
        manager.configure(apiClient: APIClient(authManager: auth))
        await manager.reconcile()
        let refreshing = expectation(description: "refresh held in flight")
        let held = HeldPushRequest()
        let log = PushRequestLog()
        PushLifecycleURLProtocol.handler = { request in
            log.append(request.request)
            if request.request.url?.path == "/api/mobile/auth/refresh" {
                held.hold(request)
                refreshing.fulfill()
            } else {
                request.respond(401)
            }
        }
        manager.didRegisterForRemoteNotifications(deviceToken: Data([1, 2]))
        await fulfillment(of: [refreshing], timeout: 3)
        auth.clearAuth()
        signIn(auth, server: "https://replacement.test", user: "bob")
        held.release(200, data: refreshResponse())
        await manager.unregister()
        XCTAssertEqual(auth.user?.id, "bob")
        XCTAssertEqual(auth.getAccessToken(), "access-bob")
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertEqual(log.snapshot().compactMap { $0.url?.path }, [
            "/api/mobile/push/verified-register", "/api/mobile/auth/refresh", "/api/mobile/push/unregister"
        ])
        XCTAssertTrue(log.snapshot().allSatisfy { $0.url?.host == "original.test" })
    }

    func testLogoutDrainsUploadBeforeUnregisterAndSessionRevocation() async {
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PushLifecycleURLProtocol.self]))
        signIn(auth)
        let api = APIClient(authManager: auth)
        let manager = PushNotificationManager(system: TestPushSystem(), relay: TestPushRelay(), storage: MemoryPushSetupStorage())
        manager.configure(apiClient: api)
        await manager.reconcile()
        let uploadStarted = expectation(description: "upload held in flight")
        let closeStarted = expectation(description: "logout closes WebSocket")
        let cleanupBeforeUpload = expectation(description: "no cleanup before upload completes")
        cleanupBeforeUpload.isInverted = true
        cleanupBeforeUpload.assertForOverFulfill = false
        let held = HeldPushRequest()
        let log = PushRequestLog()
        PushLifecycleURLProtocol.handler = { request in
            log.append(request.request)
            if request.request.url?.path == "/api/mobile/push/verified-register" {
                held.hold(request)
                uploadStarted.fulfill()
            } else {
                cleanupBeforeUpload.fulfill()
                request.respond(200)
            }
        }
        manager.didRegisterForRemoteNotifications(deviceToken: Data([1, 2]))
        await fulfillment(of: [uploadStarted], timeout: 3)
        let settings = SettingsViewModel()
        let logout = Task { @MainActor in
            await settings.logout(authManager: auth, unregisterPush: manager.unregister(context:)) {
                closeStarted.fulfill()
            }
        }
        await fulfillment(of: [closeStarted], timeout: 3)
        await fulfillment(of: [cleanupBeforeUpload], timeout: 0.2)
        // APNs callbacks arriving in the logout window must be ignored.
        manager.didRegisterForRemoteNotifications(deviceToken: Data([3, 4]))
        PushLifecycleURLProtocol.handler = { request in
            log.append(request.request)
            request.respond(200)
        }
        held.release(200)
        await logout.value
        XCTAssertEqual(log.snapshot().compactMap { $0.url?.path }, [
            "/api/mobile/push/verified-register", "/api/mobile/push/unregister", "/api/mobile/auth/revoke"
        ])
        for request in log.snapshot() {
            if request.url?.path == "/api/mobile/auth/revoke" {
                Self.assertAliceRevocation(request)
            } else {
                XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer access-alice")
            }
        }
        XCTAssertFalse(auth.isAuthenticated)
        XCTAssertNil(manager.deviceToken)
    }

    func testLogoutCompletionDoesNotClearReplacementLogin() async {
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PushLifecycleURLProtocol.self]))
        signIn(auth)
        let api = APIClient(authManager: auth)
        let manager = PushNotificationManager(system: TestPushSystem(), relay: TestPushRelay(), storage: MemoryPushSetupStorage())
        manager.configure(apiClient: api)
        await manager.reconcile()
        let unregistering = expectation(description: "unregister held in flight")
        let held = HeldPushRequest()
        let log = PushRequestLog()
        PushLifecycleURLProtocol.handler = { request in
            log.append(request.request)
            if request.request.url?.path == "/api/mobile/push/unregister" {
                held.hold(request)
                unregistering.fulfill()
            } else {
                XCTAssertEqual(request.request.url?.host, "original.test")
                XCTAssertEqual(request.request.url?.path, "/api/mobile/auth/revoke")
                Self.assertAliceRevocation(request.request)
                request.respond(200)
            }
        }
        let settings = SettingsViewModel()
        let logout = Task { @MainActor in
            await settings.logout(authManager: auth, unregisterPush: manager.unregister(context:), closeWebSocket: {})
        }
        await fulfillment(of: [unregistering], timeout: 3)
        auth.clearAuth()
        signIn(auth, server: "https://replacement.test", user: "bob")
        held.release(200)
        await logout.value
        XCTAssertEqual(auth.user?.id, "bob")
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertEqual(auth.getAccessToken(), "access-bob")
        XCTAssertEqual(log.snapshot().compactMap { $0.url?.path }, [
            "/api/mobile/push/unregister", "/api/mobile/auth/revoke"
        ])
    }
}

extension PushRegistrationLifecycleTests {
    func testLogoutKeepsCapturedIdentityIfLoginChangesWhileClosingWebSocket() async {
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PushLifecycleURLProtocol.self]))
        signIn(auth)
        let api = APIClient(authManager: auth)
        let manager = PushNotificationManager(system: TestPushSystem(), relay: TestPushRelay(), storage: MemoryPushSetupStorage())
        manager.configure(apiClient: api)
        await manager.reconcile()
        let log = PushRequestLog()
        PushLifecycleURLProtocol.handler = { request in
            log.append(request.request)
            XCTAssertEqual(request.request.url?.host, "original.test")
            if request.request.url?.path == "/api/mobile/auth/revoke" {
                Self.assertAliceRevocation(request.request)
            } else {
                XCTAssertEqual(request.request.value(forHTTPHeaderField: "Authorization"), "Bearer access-alice")
            }
            request.respond(200)
        }
        await SettingsViewModel().logout(authManager: auth, unregisterPush: manager.unregister(context:)) {
            auth.clearAuth()
            self.signIn(auth, server: "https://replacement.test", user: "bob")
            manager.configure(apiClient: api)
            await manager.reconcile()
        }
        XCTAssertEqual(auth.user?.id, "bob")
        XCTAssertTrue(auth.isAuthenticated)
        XCTAssertEqual(log.snapshot().compactMap { $0.url?.path }, [
            "/api/mobile/push/unregister", "/api/mobile/auth/revoke"
        ])
        let replacementUpload = expectation(description: "replacement login can still register")
        PushLifecycleURLProtocol.handler = { request in
            XCTAssertEqual(request.request.url?.host, "replacement.test")
            XCTAssertEqual(request.request.value(forHTTPHeaderField: "Authorization"), "Bearer access-bob")
            if request.request.url?.path == "/api/mobile/push/verified-register" { replacementUpload.fulfill() }
            request.respond(200)
        }
        manager.didRegisterForRemoteNotifications(deviceToken: Data([5, 6]))
        await fulfillment(of: [replacementUpload], timeout: 3)
        await manager.unregister()
    }
}
