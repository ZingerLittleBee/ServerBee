import Foundation
import XCTest
@testable import ServerBee

private final class TestRecoveryURLProtocol: URLProtocol {
    nonisolated(unsafe) static var handler: (@Sendable (TestRecoveryURLProtocol) -> Void)?
    private static let pending = PendingURLProtocolRequests()
    override static func canInit(with request: URLRequest) -> Bool { true }
    override static func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() { Self.pending.begin(self); Self.handler?(self) }
    override func stopLoading() { _ = Self.pending.finish(self) }
    static func cancelPending() { pending.cancelAll() }
    func respond(_ code: Int, event: String = "", outcome: String = "pending") {
        guard Self.pending.finish(self), let url = request.url,
              let response = HTTPURLResponse(url: url, statusCode: code, httpVersion: nil, headerFields: nil) else { return }
        let data = code == 200
            ? Data("{\"data\":{\"event_id\":\"\(event)\",\"outcome\":\"\(outcome)\",\"reason\":\"Fixture\",\"presentation\":\"unobserved\"}}".utf8)
            : Data("{\"error\":{\"code\":\"FIXTURE\",\"message\":\"Fixture rejection\"}}".utf8)
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }
}

private final class HeldTestRecoveryRequest: @unchecked Sendable {
    private let lock = NSLock()
    private var request: TestRecoveryURLProtocol?
    func hold(_ request: TestRecoveryURLProtocol) { lock.lock(); self.request = request; lock.unlock() }
    func release() {
        lock.lock(); let request = self.request; self.request = nil; lock.unlock()
        let event = request.map { PushSetupTestData.body($0.request)["event_id"] as? String ?? "" } ?? ""
        request?.respond(200, event: event, outcome: "accepted")
    }
}

@MainActor
final class PushTestRecoveryTests: XCTestCase {
    override func setUp() async throws { URLProtocol.registerClass(TestRecoveryURLProtocol.self) }
    override func tearDown() async throws {
        TestRecoveryURLProtocol.handler = nil
        TestRecoveryURLProtocol.cancelPending()
        URLProtocol.unregisterClass(TestRecoveryURLProtocol.self)
        AuthManager().clearAuth()
        try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
    }

    private func signedIn(_ name: String = "alice") -> AuthManager {
        let auth = AuthManager()
        auth.setServerUrl("https://serverbee.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-\(name)", accessExpiresInSecs: 900,
                                                    refreshToken: "refresh-\(name)", refreshExpiresInSecs: 3600, tokenType: "Bearer",
                                                    user: MobileUser(id: name, username: name, role: "member"),
                                                    revocationToken: "fixture-deletion-" + UUID().uuidString, mobileSessionId: UUID().uuidString))
        return auth
    }

    private func setup(_ revision: Int64) throws -> PushSetup {
        try JSONDecoder().decode(ApiResponse<PushSetup>.self, from: PushSetupTestData.response(registered: true, revision: revision)).data
    }

    private func saved(_ storage: MemoryPushSetupStorage) throws -> SavedTestPush {
        try JSONDecoder().decode(SavedTestPush.self, from: XCTUnwrap(storage.load("serverbee_pending_push_test")))
    }

    func testConfirmedNotAdmittedRebindsSameUuidToCurrentRevisionAcrossRestart() async throws {
        let auth = signedIn()
        let api = APIClient(authManager: auth)
        let storage = MemoryPushSetupStorage()
        let delivery = PushTestDelivery(storage: storage)
        let posts = AuthenticationRequestLog()
        TestRecoveryURLProtocol.handler = { request in
            if request.request.httpMethod == "GET" { request.respond(404); return }
            let count = posts.append(request.request)
            let body = PushSetupTestData.body(request.request)
            if count == 1 { request.respond(503) } else {
                request.respond(200, event: body["event_id"] as? String ?? "")
            }
        }
        delivery.configure(apiClient: api)
        await delivery.send(setup: try setup(2))
        let original = try saved(storage)
        XCTAssertEqual(original.admission, .unknown)
        XCTAssertEqual(original.request.expectedRevision, 2)
        // Registration is now confirmed at revision 3. GET404 is an
        // authoritative absence for this authenticated installation/session.
        await delivery.refresh(setup: try setup(3))
        XCTAssertEqual(try saved(storage).admission, .unadmitted)
        let restored = PushTestDelivery(storage: storage)
        restored.configure(apiClient: api)
        await restored.send(setup: try setup(3))
        XCTAssertEqual(restored.result?.outcome, "pending")
        let requests = posts.snapshot()
        XCTAssertEqual(requests.count, 2)
        XCTAssertEqual(PushSetupTestData.body(requests[0])["event_id"] as? String, original.request.eventId)
        XCTAssertEqual(PushSetupTestData.body(requests[1])["event_id"] as? String, original.request.eventId)
        XCTAssertEqual((PushSetupTestData.body(requests[1])["expected_revision"] as? NSNumber)?.int64Value, 3)
        XCTAssertEqual(try saved(storage).admission, .admitted)
    }

    func testStaleRevisionRejectionRecoversOnlyAfterOwned404UsingSameUuid() async throws {
        let auth = signedIn()
        let api = APIClient(authManager: auth)
        let storage = MemoryPushSetupStorage()
        let delivery = PushTestDelivery(storage: storage)
        let posts = AuthenticationRequestLog()
        let reads = AuthenticationRequestLog()
        TestRecoveryURLProtocol.handler = { request in
            if request.request.httpMethod == "GET" {
                _ = reads.append(request.request)
                request.respond(404)
            } else if posts.append(request.request) == 1 { request.respond(409) } else {
                request.respond(200, event: PushSetupTestData.body(request.request)["event_id"] as? String ?? "")
            }
        }
        delivery.configure(apiClient: api)
        await delivery.send(setup: try setup(2))
        let event = try saved(storage).request.eventId
        XCTAssertEqual(try saved(storage).admission, .unknown)
        await delivery.send(setup: try setup(3))
        XCTAssertEqual(reads.snapshot().count, 1)
        let requests = posts.snapshot()
        XCTAssertEqual(requests.count, 2)
        XCTAssertEqual(PushSetupTestData.body(requests[1])["event_id"] as? String, event)
        XCTAssertEqual((PushSetupTestData.body(requests[1])["expected_revision"] as? NSNumber)?.int64Value, 3)
        XCTAssertEqual(delivery.result?.outcome, "pending")
        XCTAssertEqual(try saved(storage).admission, .admitted)
    }

    func testUnknownAcceptedReplyIsQueriedWithoutEnqueueAfterRevisionChangeAndRestart() async throws {
        let auth = signedIn()
        let api = APIClient(authManager: auth)
        let storage = MemoryPushSetupStorage()
        let posts = AuthenticationRequestLog()
        TestRecoveryURLProtocol.handler = { request in
            if request.request.httpMethod == "POST" {
                _ = posts.append(request.request)
                // The Server admitted and accepted the test, but its reply was lost.
                request.respond(503)
            } else {
                request.respond(200, event: request.request.url?.lastPathComponent ?? "", outcome: "accepted")
            }
        }
        let delivery = PushTestDelivery(storage: storage)
        delivery.configure(apiClient: api)
        await delivery.send(setup: try setup(2))
        let prior = try saved(storage)
        let event = prior.request.eventId
        // Metadata from the first implementation omitted admission state.
        try storage.save(JSONEncoder().encode(SavedTestPush(scope: prior.scope, request: prior.request, admission: nil)), key: "serverbee_pending_push_test")
        let restored = PushTestDelivery(storage: storage)
        restored.configure(apiClient: api)
        await restored.send(setup: try setup(3))
        XCTAssertEqual(posts.snapshot().count, 1)
        XCTAssertEqual(restored.result?.eventId, event)
        XCTAssertEqual(restored.result?.outcome, "accepted")
        XCTAssertEqual(restored.result?.presentation, "unobserved")
        XCTAssertEqual(try saved(storage).request.expectedRevision, 2)
        XCTAssertEqual(try saved(storage).admission, .admitted)
    }

    func testKnownAdmittedWorkIsNeverReenqueuedOnMissingStatus() async throws {
        let auth = signedIn()
        let api = APIClient(authManager: auth)
        let storage = MemoryPushSetupStorage()
        let posts = AuthenticationRequestLog()
        TestRecoveryURLProtocol.handler = { request in
            if request.request.httpMethod == "POST" {
                _ = posts.append(request.request)
                request.respond(200, event: PushSetupTestData.body(request.request)["event_id"] as? String ?? "")
            } else { request.respond(404) }
        }
        let delivery = PushTestDelivery(storage: storage)
        delivery.configure(apiClient: api)
        await delivery.send(setup: try setup(2))
        let event = try saved(storage).request.eventId
        let restored = PushTestDelivery(storage: storage)
        restored.configure(apiClient: api)
        await restored.send(setup: try setup(3))
        XCTAssertEqual(posts.snapshot().count, 1)
        XCTAssertNotNil(restored.errorMessage)
        XCTAssertEqual(try saved(storage).request.eventId, event)
        XCTAssertEqual(try saved(storage).admission, .admitted)
    }

    func testConflictingLateAdmissionIsResolvedByLookupWithoutCreatingAnotherUuid() async throws {
        let auth = signedIn()
        let api = APIClient(authManager: auth)
        let storage = MemoryPushSetupStorage()
        let posts = AuthenticationRequestLog()
        let reads = AuthenticationRequestLog()
        TestRecoveryURLProtocol.handler = { request in
            if request.request.httpMethod == "POST" {
                let count = posts.append(request.request)
                request.respond(count == 1 ? 503 : 409)
            } else if reads.append(request.request) == 1 { request.respond(404) } else { request.respond(200, event: request.request.url?.lastPathComponent ?? "") }
        }
        let delivery = PushTestDelivery(storage: storage)
        delivery.configure(apiClient: api)
        await delivery.send(setup: try setup(2))
        let event = try saved(storage).request.eventId
        await delivery.send(setup: try setup(3))
        XCTAssertEqual(try saved(storage).request.eventId, event)
        XCTAssertEqual(try saved(storage).admission, .unknown)
        await delivery.refresh(setup: try setup(3))
        XCTAssertEqual(delivery.result?.eventId, event)
        XCTAssertEqual(posts.snapshot().count, 2)
        XCTAssertEqual(try saved(storage).admission, .admitted)
    }

    func testLatePreviousAccountCompletionCannotOverwriteReplacementOrRestoredStorage() async throws {
        let auth = signedIn()
        let api = APIClient(authManager: auth)
        let storage = MemoryPushSetupStorage()
        let delivery = PushTestDelivery(storage: storage)
        let held = HeldTestRecoveryRequest()
        let started = expectation(description: "previous account POST held at HTTP boundary")
        TestRecoveryURLProtocol.handler = { request in
            if request.request.value(forHTTPHeaderField: "Authorization") == "Bearer fixture-alice" {
                held.hold(request); started.fulfill()
            } else {
                request.respond(200, event: PushSetupTestData.body(request.request)["event_id"] as? String ?? "")
            }
        }
        delivery.configure(apiClient: api)
        let old = Task { await delivery.send(setup: try? setup(2)) }
        await fulfillment(of: [started], timeout: 3)
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-bob", accessExpiresInSecs: 900,
                                                    refreshToken: "refresh-bob", refreshExpiresInSecs: 3600, tokenType: "Bearer",
                                                    user: MobileUser(id: "bob", username: "bob", role: "member"),
                                                    revocationToken: "fixture-deletion-" + UUID().uuidString, mobileSessionId: UUID().uuidString))
        delivery.configure(apiClient: api)
        await delivery.send(setup: try setup(3))
        let replacement = try saved(storage)
        held.release()
        await old.value
        XCTAssertEqual(delivery.result?.eventId, replacement.request.eventId)
        XCTAssertEqual(delivery.result?.outcome, "pending")
        XCTAssertEqual(try saved(storage).scope, replacement.scope)
        XCTAssertEqual(try saved(storage).request.eventId, replacement.request.eventId)
        XCTAssertFalse(delivery.isTesting)
        let restored = PushTestDelivery(storage: storage)
        restored.configure(apiClient: api)
        XCTAssertNil(restored.result)
        XCTAssertEqual(try saved(storage).scope, try XCTUnwrap(auth.captureContext()).pushScope)
    }
}
