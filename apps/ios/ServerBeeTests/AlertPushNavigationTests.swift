import CryptoKit
import Foundation
import UserNotifications
import XCTest
@testable import ServerBee

private struct EncryptedAlertFixture {
    let envelope: PushEnvelope
    let key: PushContentKey
    let target: String
}

@MainActor
final class AlertPushNavigationTests: XCTestCase {
    private func context(user: String = "alice", server: String = "https://serverbee.test") -> MobileAuthenticationContext {
        MobileAuthenticationContext(serverUrl: server, userId: user, installationId: "alert-install", generation: UUID(),
                                    accessToken: "fixture-access", revocationToken: "fixture-proof", refreshToken: "fixture-refresh")
    }

    private func encrypted(status: String = "firing", alertKey: String? = nil) throws -> EncryptedAlertFixture {
        let current = context()
        let now = Int64(Date().timeIntervalSince1970)
        let parts = ["rule-1", "server-1", "203.0.113.7", "2033-05-18T03:33:20+00:00"]
        let encoded = try JSONEncoder().encode(parts).base64EncodedString()
            .replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
        let target = alertKey ?? "v1.\(encoded)"
        let content = PushContent(kind: "alert", deploymentId: current.serverUrl, userId: current.userId,
                                  installationId: current.installationId, eventId: UUID().uuidString.lowercased(), createdAt: now, expiresAt: now + 1800,
                                  alert: AlertPushTarget(alertKey: target, status: status, ruleName: "High CPU", serverName: "vps-a"))
        let key = PushContentKey(keyId: UUID().uuidString.lowercased(), key: Data(repeating: 7, count: 32).base64EncodedString(),
                                 deploymentId: current.serverUrl, userId: current.userId, installationId: current.installationId, scope: current.pushScope)
        let identity = try content.identity
        let sealed = try AES.GCM.seal(JSONEncoder().encode(content), using: SymmetricKey(data: try XCTUnwrap(Data(base64Encoded: key.key))),
                                      authenticating: Data("ServerBee.Push.v1|\(key.keyId)|\(identity)".utf8))
        let envelope = PushEnvelope(version: 1, keyId: key.keyId, identity: identity,
                                    nonce: sealed.nonce.withUnsafeBytes { Data($0) }.base64EncodedString(),
                                    ciphertext: (sealed.ciphertext + sealed.tag).base64EncodedString())
        return EncryptedAlertFixture(envelope: envelope, key: key, target: target)
    }

    func testAlertColdTapRoutesCompleteIdentityAfterDependencyWiring() throws {
        let fixture = try encrypted()
        let envelope = fixture.envelope
        let key = fixture.key
        let target = fixture.target
        let delegate = AppDelegate()
        delegate.bufferNotification(userInfo: ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))])
        let router = PushNotificationRouter()
        delegate.pushRouter = router
        XCTAssertNotNil(router.pendingEnvelope)
        let link = try XCTUnwrap(router.consumeAccountTarget(context: context(), key: key))
        XCTAssertEqual(link, .alertDetail(alertKey: target))
        var tab = 0
        var servers: [ServerNavigationTarget] = []
        var alerts: [ServerDeepLink] = []
        ContentView.applyDeepLink(link, selectedTab: &tab, serversPath: &servers, alertsPath: &alerts)
        XCTAssertEqual(tab, 1)
        XCTAssertEqual(alerts, [.alertDetail(alertKey: target)])
        XCTAssertNil(router.pendingEnvelope)
        for replacement in [context(user: "bob"), context(server: "https://other.test")] {
            router.enqueue(envelope: envelope)
            XCTAssertNil(router.consumeAccountTarget(context: replacement, key: key))
        }
    }

    func testTriggerAndRecoveryRenderLocalizedCopyAndRejectIncompleteTargets() throws {
        for status in ["firing", "resolved"] {
            let fixture = try encrypted(status: status)
            let envelope = fixture.envelope
            let key = fixture.key
            let input = UNMutableNotificationContent()
            input.title = "Untrusted plaintext"
            input.userInfo = ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))]
            let rendered = PushNotificationRenderer.render(input, key: key)
            XCTAssertEqual(rendered.title, status == "firing" ? String(localized: "Alert triggered") : String(localized: "Alert recovered"))
            XCTAssertEqual(rendered.body, String(format: String(localized: "%@ on %@"), "High CPU", "vps-a"))
            XCTAssertNotNil(rendered.userInfo["serverbee_target"])
            let service = NotificationService()
            service.loadKey = { key }
            var completions = 0
            service.didReceive(UNNotificationRequest(identifier: status, content: input, trigger: nil)) { result in
                completions += 1
                XCTAssertEqual(result.title, rendered.title)
                XCTAssertEqual(result.body, rendered.body)
            }
            service.serviceExtensionTimeWillExpire()
            XCTAssertEqual(completions, 1)
        }
        for (status, target) in [("firing", "rule:server"), ("unknown", "v1.invalid")] {
            let fixture = try encrypted(status: status, alertKey: target)
            let envelope = fixture.envelope
            let key = fixture.key
            XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(envelope, key: key))
            let router = PushNotificationRouter()
            router.enqueue(envelope: envelope)
            XCTAssertNil(router.consumeAccountTarget(context: context(), key: key))
        }
    }

    func testListDecodesCompleteKeyWithoutLosingLegacyCompatibility() throws {
        let bytes = Data("""
        {"alert_key":"v1.complete-key","rule_id":"r","rule_name":"Rule","server_id":"s","server_name":"Server",
         "status":"firing","event_at":"2033-05-18T03:33:20Z","count":1}
        """.utf8)
        let event = try JSONDecoder.snakeCase.decode(MobileAlertEvent.self, from: bytes)
        XCTAssertEqual(event.alertKey, "v1.complete-key")
    }

    func testRustAlertVectorDecryptsThroughSharedRenderer() throws {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "push-alert-envelope-v1", withExtension: "json"))
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
        let expected = try JSONDecoder().decode(PushContent.self, from: JSONSerialization.data(withJSONObject: try XCTUnwrap(object["content"])))
        let envelope = try JSONDecoder().decode(PushEnvelope.self, from: JSONSerialization.data(withJSONObject: try XCTUnwrap(object["envelope"])))
        let key = PushContentKey(keyId: envelope.keyId, key: try XCTUnwrap(object["key"] as? String), deploymentId: expected.deploymentId,
                                 userId: expected.userId, installationId: expected.installationId, scope: "fixture")
        XCTAssertEqual(try PushEnvelopeDecoder.decrypt(envelope, key: key, now: expected.createdAt), expected)
        let input = UNMutableNotificationContent()
        input.userInfo = ["serverbee_envelope": try XCTUnwrap(object["envelope"])]
        let rendered = PushNotificationRenderer.render(input, key: key, now: expected.createdAt)
        XCTAssertEqual(rendered.title, String(localized: "Alert triggered"))
        XCTAssertNotNil(rendered.userInfo["serverbee_target"])
    }
}
