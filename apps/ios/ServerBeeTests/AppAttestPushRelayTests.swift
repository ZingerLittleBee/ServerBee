import DeviceCheck
import Foundation
import XCTest
@testable import ServerBee

@MainActor
private final class NativeAttestFixture: AppAttestBoundary {
    var supported = true
    var keys = 0
    var assertions: [String] = []
    var attestationHashes: [Data] = []
    var assertionError: Error?
    var attestationError: Error?
    var held: CheckedContinuation<Void, Error>?
    var suspendAssertion: (() -> Void)?

    func generateKey() async throws -> String { keys += 1; return "key-\(keys)" }
    func attestKey(_ key: String, hash: Data) async throws -> Data {
        attestationHashes.append(hash)
        if let attestationError { throw attestationError }
        return Data(key.utf8)
    }
    func assertion(_ key: String, hash: Data) async throws -> Data {
        assertions.append(key)
        if let assertionError { throw assertionError }
        if let suspendAssertion {
            try await withCheckedThrowingContinuation { held = $0; suspendAssertion() }
        }
        return Data(key.utf8)
    }
    func release(_ error: Error? = nil) {
        let continuation = held
        held = nil
        suspendAssertion = nil
        if let error { continuation?.resume(throwing: error) } else { continuation?.resume() }
    }
}

@MainActor
private final class RelayHTTPFixture: PushRelayTransport {
    var challenges: [String: [String: Any]] = [:]
    var active: [String: String] = [:]
    var actions: [String] = []
    var held: CheckedContinuation<Void, Error>?
    var suspendChallenge: (() -> Void)?
    var proofError: Error?

    func send(_ path: String, body: Data, relayUrl: String) async throws -> Data {
        let value = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        if path == "/v1/challenges" {
            let id = UUID().uuidString
            challenges[id] = value
            if let suspendChallenge {
                try await withCheckedThrowingContinuation { held = $0; suspendChallenge() }
            }
            return try JSONSerialization.data(withJSONObject: ["challenge_id": id, "client_data": Data(id.utf8).base64EncodedString()])
        }
        if let proofError { throw proofError }
        let id = try XCTUnwrap(value["challenge_id"] as? String)
        let scope = try XCTUnwrap(challenges.removeValue(forKey: id))
        let key = try XCTUnwrap(scope["key_id"] as? String)
        actions.append(path)
        if path == "/v1/revoke" {
            if active[key] == scope["grant_id"] as? String { active[key] = nil }
            return Data(#"{"revoked":true}"#.utf8)
        }
        let grant = UUID().uuidString
        active[key] = grant
        return try JSONEncoder.snakeCase.encode(RelayGrant(
            grantId: grant, grantToken: "fixture-\(grant)", keyId: key,
            deviceToken: try XCTUnwrap(scope["device_token"] as? String), environment: "sandbox", expiresAt: 2_000_000_000
        ))
    }
    func release(_ error: Error? = nil) {
        let continuation = held
        held = nil
        suspendChallenge = nil
        if let error { continuation?.resume(throwing: error) } else { continuation?.resume() }
    }
}

@MainActor
final class AppAttestPushRelayTests: XCTestCase {
    private func error(_ code: DCError.Code) -> NSError {
        NSError(domain: DCError.errorDomain, code: code.rawValue)
    }

    func testInvalidLocalKeyRetriesWithFreshAttestationAndKeepsOtherLoginKey() async throws {
        let native = NativeAttestFixture()
        let http = RelayHTTPFixture()
        let relay = AppAttestPushRelay(service: native, transport: http, storage: MemoryPushSetupStorage(), environment: "sandbox")
        let old = try await relay.register(token: "token", relayUrl: "https://relay.test", scope: "alice-login") {}
        let other = try await relay.register(token: "token", relayUrl: "https://relay.test", scope: "bob-login") {}
        native.assertionError = error(.invalidKey)
        do {
            _ = try await relay.register(token: "token", relayUrl: "https://relay.test", scope: "alice-login") {}
            XCTFail("Invalid local key must be surfaced")
        } catch { XCTAssertEqual((error as NSError).code, DCError.Code.invalidKey.rawValue) }
        native.assertionError = nil
        let recovered = try await relay.register(token: "token", relayUrl: "https://relay.test", scope: "alice-login") {}
        XCTAssertNotEqual(recovered.keyId, old.keyId)
        XCTAssertEqual(http.actions, ["/v1/attest", "/v1/attest", "/v1/attest"])
        XCTAssertEqual(http.active[other.keyId], other.grantId)
        XCTAssertEqual(native.keys, 3)
    }

    func testTransientAssertionAndLostRelayResponseRetainAdmittedKeyForRetry() async throws {
        let native = NativeAttestFixture()
        let http = RelayHTTPFixture()
        let relay = AppAttestPushRelay(service: native, transport: http, storage: MemoryPushSetupStorage(), environment: "sandbox")
        let initial = try await relay.register(token: "token", relayUrl: "https://relay.test", scope: "login") {}
        for failure in ["native", "HTTP"] {
            if failure == "native" { native.assertionError = URLError(.timedOut) } else { http.proofError = URLError(.networkConnectionLost) }
            do {
                _ = try await relay.register(token: "token", relayUrl: "https://relay.test", scope: "login") {}
                XCTFail("Transient failure must be surfaced")
            } catch { XCTAssertEqual((error as NSError).domain, NSURLErrorDomain) }
            native.assertionError = nil
            http.proofError = nil
            let renewed = try await relay.register(token: "token", relayUrl: "https://relay.test", scope: "login") {}
            XCTAssertEqual(renewed.keyId, initial.keyId)
        }
        XCTAssertEqual(native.keys, 1)
    }

    func testAppleServerUnavailableKeepsUnattestedKeyForRetry() async throws {
        let native = NativeAttestFixture()
        let http = RelayHTTPFixture()
        let relay = AppAttestPushRelay(service: native, transport: http, storage: MemoryPushSetupStorage(), environment: "sandbox")
        native.attestationError = error(.serverUnavailable)
        do {
            _ = try await relay.register(token: "token", relayUrl: "https://relay.test", scope: "login") {}
            XCTFail("Apple outage must be surfaced")
        } catch { XCTAssertEqual((error as NSError).code, DCError.Code.serverUnavailable.rawValue) }
        native.attestationError = nil
        let grant = try await relay.register(token: "token", relayUrl: "https://relay.test", scope: "login") {}
        XCTAssertEqual(native.keys, 1)
        XCTAssertEqual(grant.keyId, "key-1")
        XCTAssertEqual(native.attestationHashes.count, 2)
        XCTAssertEqual(native.attestationHashes.first, native.attestationHashes.last)
    }

    func testReplacementLoginsCannotBeRevokedByHeldOldChallengeOrAssertion() async throws {
        for sameAccount in [false, true] {
            for phase in ["challenge", "assertion"] { try await replacement(sameAccount: sameAccount, phase: phase) }
        }
    }

    private func replacement(sameAccount: Bool, phase: String) async throws {
        let native = NativeAttestFixture()
        let http = RelayHTTPFixture()
        defer { native.release(CancellationError()); http.release(CancellationError()) }
        let storage = MemoryPushSetupStorage()
        let relay = AppAttestPushRelay(service: native, transport: http, storage: storage, environment: "sandbox")
        let auth = AuthManager()
        defer { auth.clearAuth() }
        func login(_ user: String, proof: String) {
            auth.setServerUrl("https://deployment.test")
            auth.handleLoginResponse(MobileTokenResponse(
                accessToken: "access-\(proof)", accessExpiresInSecs: 900, refreshToken: "refresh-\(proof)", refreshExpiresInSecs: 3600,
                tokenType: "Bearer", user: MobileUser(id: user, username: user, role: "member"), revocationToken: proof
            ))
        }
        login("alice", proof: "old-login")
        let old = try XCTUnwrap(auth.captureContext())
        _ = try await relay.register(token: "token", relayUrl: "https://relay.test", scope: old.pushScope) {}
        let held = expectation(description: "old \(phase) suspended")
        if phase == "challenge" { http.suspendChallenge = { held.fulfill() } } else { native.suspendAssertion = { held.fulfill() } }
        let pending = Task {
            try await relay.register(token: "token", relayUrl: "https://relay.test", scope: old.pushScope) {
                guard auth.isCurrent(old) else { throw AuthError.staleIdentity }
            }
        }
        await fulfillment(of: [held], timeout: 3)
        // Only the old in-flight operation remains held; the replacement can finish.
        http.suspendChallenge = nil
        native.suspendAssertion = nil
        auth.clearAuth()
        login(sameAccount ? "alice" : "bob", proof: "replacement-login")
        let current = try XCTUnwrap(auth.captureContext())
        XCTAssertNotEqual(current.pushScope, old.pushScope)
        let replacement = try await relay.register(token: "token", relayUrl: "https://relay.test", scope: current.pushScope) {}
        http.release()
        native.release()
        do { _ = try await pending.value; XCTFail("Old login must fail before the Relay mutation") } catch AuthError.staleIdentity {
            // Expected even for same-account login replacement.
        }
        XCTAssertEqual(http.active[replacement.keyId], replacement.grantId)
        XCTAssertEqual(http.actions, ["/v1/attest", "/v1/attest"])
        XCTAssertTrue(auth.isCurrent(current))
    }

    func testCancelledFixtureReleasesSuspendedNativeWork() async throws {
        let native = NativeAttestFixture()
        let http = RelayHTTPFixture()
        defer { native.release(CancellationError()); http.release(CancellationError()) }
        let relay = AppAttestPushRelay(service: native, transport: http, storage: MemoryPushSetupStorage(), environment: "sandbox")
        let entered = expectation(description: "challenge entered")
        http.suspendChallenge = { entered.fulfill() }
        let operation = Task { try await relay.register(token: "token", relayUrl: "https://relay.test", scope: "login") {} }
        await fulfillment(of: [entered], timeout: 3)
        http.release(CancellationError())
        do { _ = try await operation.value; XCTFail("Cancellation must end the operation") } catch is CancellationError { }
        XCTAssertNil(http.held)
        XCTAssertTrue(http.actions.isEmpty)
    }
}
