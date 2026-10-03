import CryptoKit
import Foundation
import XCTest
@testable import ServerBee

@MainActor
final class EncryptedPushNavigationTests: XCTestCase {
    private func context(user: String = "alice", deployment: String = "https://serverbee.test") -> MobileAuthenticationContext {
        MobileAuthenticationContext(serverUrl: deployment, userId: user, installationId: "test-install", generation: UUID(),
                                    accessToken: "fixture-access", revocationToken: "fixture-proof", refreshToken: "fixture-refresh")
    }
    private func encrypted(_ context: MobileAuthenticationContext) throws -> (PushEnvelope, PushContentKey) {
        let now = Int64(Date().timeIntervalSince1970)
        let content = PushContent(kind: "test", deploymentId: context.serverUrl, userId: context.userId, installationId: context.installationId,
                                  eventId: UUID().uuidString.lowercased(), createdAt: now, expiresAt: now + 1800)
        let key = PushContentKey(keyId: UUID().uuidString.lowercased(), key: Data(repeating: 7, count: 32).base64EncodedString(),
                                 deploymentId: context.serverUrl, userId: context.userId, installationId: context.installationId, scope: context.pushScope)
        let identity = try content.identity
        let aad = Data("ServerBee.Push.v1|\(key.keyId)|\(identity)".utf8)
        let sealed = try AES.GCM.seal(JSONEncoder().encode(content), using: SymmetricKey(data: try XCTUnwrap(Data(base64Encoded: key.key))), authenticating: aad)
        let nonce = sealed.nonce.withUnsafeBytes { Data($0) }
        return (PushEnvelope(version: 1, keyId: key.keyId, identity: identity, nonce: nonce.base64EncodedString(),
                             ciphertext: (sealed.ciphertext + sealed.tag).base64EncodedString()), key)
    }

    func testEarlyDelegateTapWaitsForStoresAndRejectsReplacementAccountDeploymentAndLogin() throws {
        let current = context()
        let (envelope, key) = try encrypted(current)
        let delegate = AppDelegate()
        let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))
        delegate.bufferNotification(userInfo: ["serverbee_envelope": object])
        let router = PushNotificationRouter()
        delegate.pushRouter = router
        XCTAssertNotNil(router.pendingEnvelope)
        XCTAssertEqual(router.consumeAccountTarget(context: current, key: key), .account)
        XCTAssertNil(router.pendingEnvelope)
        for mismatch in [context(user: "bob"), context(deployment: "https://other.test"),
                         MobileAuthenticationContext(serverUrl: current.serverUrl, userId: current.userId, installationId: current.installationId,
                                                     generation: UUID(), accessToken: "new", revocationToken: "new-login-proof", refreshToken: "new-refresh")] {
            router.enqueue(envelope: envelope)
            XCTAssertNil(router.consumeAccountTarget(context: mismatch, key: key))
        }
        router.enqueue(envelope: envelope)
        XCTAssertNil(router.consumeAccountTarget(context: current, key: nil))
        delegate.bufferNotification(userInfo: ["server_id": "victim", "serverbee_target": ["user_id": "alice"]])
        XCTAssertNil(router.pendingEnvelope)
    }

    func testTestTapOpensCurrentAccount() {
        var tab = 0
        var servers: [ServerNavigationTarget] = [.detailById("old")]
        var alerts: [ServerDeepLink] = [.alertDetail(alertKey: "old")]
        ContentView.applyDeepLink(.account, selectedTab: &tab, serversPath: &servers, alertsPath: &alerts)
        XCTAssertEqual(tab, 3)
        XCTAssertTrue(servers.isEmpty)
        XCTAssertTrue(alerts.isEmpty)
    }

    func testStitchedServerRelayCiphertextColdTapValidatesCurrentAccount() throws {
        guard let directory = ProcessInfo.processInfo.environment["SERVERBEE_PUSH_TRACE_DIR"], !directory.isEmpty else {
            throw XCTSkip("Run real Server/Relay trace and pass TEST_RUNNER_SERVERBEE_PUSH_TRACE_DIR")
        }
        let trace = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: directory).appendingPathComponent("trace.json"))) as? [String: Any])
        let registration = try XCTUnwrap(trace["registration"] as? [String: Any])
        let payload = try XCTUnwrap(trace["payload"] as? [String: Any])
        let current = MobileAuthenticationContext(serverUrl: try XCTUnwrap(registration["deployment_id"] as? String),
                                                  userId: try XCTUnwrap(trace["user_id"] as? String), installationId: try XCTUnwrap(trace["installation_id"] as? String),
                                                  generation: UUID(), accessToken: "fixture-access", revocationToken: "fixture-proof", refreshToken: "fixture-refresh")
        let key = PushContentKey(keyId: try XCTUnwrap(registration["content_key_id"] as? String), key: try XCTUnwrap(registration["content_key"] as? String),
                                 deploymentId: current.serverUrl, userId: current.userId, installationId: current.installationId, scope: current.pushScope)
        let delegate = AppDelegate()
        delegate.bufferNotification(userInfo: payload)
        let router = PushNotificationRouter()
        delegate.pushRouter = router
        XCTAssertEqual(router.consumeAccountTarget(context: current, key: key), .account)
    }
}
