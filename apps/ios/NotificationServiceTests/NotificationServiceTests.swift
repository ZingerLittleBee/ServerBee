import CryptoKit
import Foundation
import UserNotifications
import XCTest
@testable import ServerBee

final class NotificationServiceTests: XCTestCase {
    private func vector(resource: String = "push-envelope-v1") throws -> NotificationEnvelopeFixture {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(forResource: resource, withExtension: "json"))
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
        let content = try JSONDecoder().decode(PushContent.self, from: JSONSerialization.data(withJSONObject: try XCTUnwrap(object["content"])))
        let envelope = try JSONDecoder().decode(PushEnvelope.self, from: JSONSerialization.data(withJSONObject: try XCTUnwrap(object["envelope"])))
        let key = PushContentKey(keyId: envelope.keyId, key: try XCTUnwrap(object["key"] as? String), deploymentId: content.deploymentId,
                                 userId: content.userId, installationId: content.installationId, scope: "fixture")
        return NotificationEnvelopeFixture(envelope: envelope, key: key, content: content)
    }

    func testRustVectorDecryptsAndRejectsTamperingVersionIdentityWrongKeyAndOversize() throws {
        let fixture = try vector()
        let envelope = fixture.envelope
        let key = fixture.key
        let expected = fixture.content
        XCTAssertEqual(try PushEnvelopeDecoder.decrypt(envelope, key: key, now: expected.createdAt), expected)
        let candidates = [
            PushEnvelope(version: 2, keyId: envelope.keyId, identity: envelope.identity, nonce: envelope.nonce, ciphertext: envelope.ciphertext),
            PushEnvelope(version: 1, keyId: envelope.keyId, identity: String(repeating: "0", count: 64), nonce: envelope.nonce, ciphertext: envelope.ciphertext),
            PushEnvelope(version: 1, keyId: envelope.keyId, identity: envelope.identity, nonce: envelope.nonce, ciphertext: "AAAA" + envelope.ciphertext.dropFirst(4)),
            PushEnvelope(version: 1, keyId: envelope.keyId, identity: envelope.identity, nonce: envelope.nonce, ciphertext: String(repeating: "A", count: 2800))
        ]
        for candidate in candidates { XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(candidate, key: key, now: expected.createdAt)) }
        let wrong = PushContentKey(keyId: key.keyId, key: Data(repeating: 0, count: 32).base64EncodedString(), deploymentId: key.deploymentId,
                                   userId: key.userId, installationId: key.installationId, scope: key.scope)
        XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(envelope, key: wrong, now: expected.createdAt))
        let identityMismatch = PushContentKey(keyId: key.keyId, key: key.key, deploymentId: "https://other.test", userId: key.userId,
                                              installationId: key.installationId, scope: key.scope)
        XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(envelope, key: identityMismatch, now: expected.createdAt))
        XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(envelope, key: key, now: expected.expiresAt))
        let input = UNMutableNotificationContent()
        input.title = "Sensitive plaintext must not survive"
        input.userInfo = ["server_id": "victim", "serverbee_envelope": ["version": 2]]
        let fallback = PushNotificationRenderer.render(input, key: key, now: expected.createdAt)
        XCTAssertEqual(fallback.title, "ServerBee")
        XCTAssertTrue(fallback.userInfo.isEmpty)
        XCTAssertNotEqual(fallback.body, input.body)
    }

    func testActualExtensionFallbackAndExpiryCompleteOnce() {
        let service = NotificationService()
        service.loadKey = { nil }
        let input = UNMutableNotificationContent()
        input.title = "Untrusted title"
        input.userInfo = ["server_id": "victim"]
        var completions = 0
        service.didReceive(UNNotificationRequest(identifier: "fallback", content: input, trigger: nil)) { content in
            completions += 1
            XCTAssertEqual(content.title, "ServerBee")
            XCTAssertTrue(content.userInfo.isEmpty)
        }
        service.serviceExtensionTimeWillExpire()
        XCTAssertEqual(completions, 1)
    }

    func testSecurityVectorAndTampering() throws {
        let fixture = try vector(resource: "push-security-envelope-v1")
        let envelope = fixture.envelope
        let key = fixture.key
        let content = fixture.content
        XCTAssertEqual(try PushEnvelopeDecoder.decrypt(envelope, key: key, now: content.createdAt), content)
        let input = UNMutableNotificationContent()
        input.userInfo = ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))]
        let rendered = PushNotificationRenderer.render(input, key: key, now: content.createdAt)
        XCTAssertEqual(rendered.title, String(localized: "Security rule matched"))
        XCTAssertEqual(rendered.body, String(localized: "SSH brute-force activity matched a security rule."))
        let target = try XCTUnwrap(rendered.userInfo["serverbee_target"] as? [String: Any])
        XCTAssertEqual(target["server_id"] as? String, content.serverId)
        XCTAssertEqual(target["security_event_id"] as? String, content.eventId)
        let tampered = PushEnvelope(version: 1, keyId: envelope.keyId, identity: envelope.identity, nonce: envelope.nonce,
                                    ciphertext: "AAAA" + envelope.ciphertext.dropFirst(4))
        input.userInfo = ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(tampered))]
        XCTAssertTrue(PushNotificationRenderer.render(input, key: key, now: content.createdAt).userInfo.isEmpty)
    }

    func testExtensionSecurityTypesAndFallback() throws {
        let fixture = try vector(resource: "push-security-envelope-v1")
        let key = fixture.key
        let expected = fixture.content
        let now = Int64(Date().timeIntervalSince1970)
        let secret = SymmetricKey(data: try XCTUnwrap(Data(base64Encoded: key.key)))
        for eventType in ["ssh_login", "ssh_brute_force", "port_scan", "unsupported", "missing-server"] {
            let content = PushContent(kind: "security", deploymentId: expected.deploymentId, userId: expected.userId,
                                      installationId: expected.installationId, eventId: expected.eventId, createdAt: now, expiresAt: now + 1800,
                                      serverId: eventType == "missing-server" ? nil : expected.serverId,
                                      securityEventId: expected.eventId, securityEventType: eventType)
            let sealed = try AES.GCM.seal(JSONEncoder().encode(content), using: secret,
                                          authenticating: Data("ServerBee.Push.v1|\(key.keyId)|\(fixture.envelope.identity)".utf8))
            let envelope = PushEnvelope(version: 1, keyId: key.keyId, identity: fixture.envelope.identity,
                                        nonce: sealed.nonce.withUnsafeBytes { Data($0) }.base64EncodedString(),
                                        ciphertext: (sealed.ciphertext + sealed.tag).base64EncodedString())
            let input = UNMutableNotificationContent()
            input.userInfo = ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(envelope))]
            let service = NotificationService()
            service.loadKey = { key }
            var completions = 0
            service.didReceive(UNNotificationRequest(identifier: eventType, content: input, trigger: nil)) { rendered in
                completions += 1
                if ["unsupported", "missing-server"].contains(eventType) {
                    XCTAssertEqual(rendered.title, "ServerBee")
                    XCTAssertTrue(rendered.userInfo.isEmpty)
                } else {
                    XCTAssertEqual(rendered.title, String(localized: "Security rule matched"))
                    XCTAssertEqual((rendered.userInfo["serverbee_target"] as? [String: Any])?["security_event_type"] as? String, eventType)
                }
            }
            service.serviceExtensionTimeWillExpire()
            XCTAssertEqual(completions, 1)
        }
    }

    func testStitchedServerRelayPayloadThroughActualNotificationExtension() throws {
        guard let directory = ProcessInfo.processInfo.environment["SERVERBEE_PUSH_TRACE_DIR"], !directory.isEmpty else {
            throw XCTSkip("Run the real Server/Relay trace command first and pass TEST_RUNNER_SERVERBEE_PUSH_TRACE_DIR")
        }
        let data = try Data(contentsOf: URL(fileURLWithPath: directory).appendingPathComponent("trace.json"))
        let trace = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let registration = try XCTUnwrap(trace["registration"] as? [String: Any])
        let payload = try XCTUnwrap(trace["payload"] as? [String: Any])
        let key = PushContentKey(keyId: try XCTUnwrap(registration["content_key_id"] as? String), key: try XCTUnwrap(registration["content_key"] as? String),
                                 deploymentId: try XCTUnwrap(registration["deployment_id"] as? String), userId: try XCTUnwrap(trace["user_id"] as? String),
                                 installationId: try XCTUnwrap(trace["installation_id"] as? String), scope: "fixture")
        let input = UNMutableNotificationContent()
        input.userInfo = payload
        let service = NotificationService()
        service.loadKey = { key }
        var completions = 0
        service.didReceive(UNNotificationRequest(identifier: "stitched", content: input, trigger: nil)) { rendered in
            completions += 1
            XCTAssertNotEqual(rendered.body, "Open ServerBee to view this notification.")
            XCTAssertEqual(rendered.userInfo["serverbee_key_id"] as? String, key.keyId)
            XCTAssertEqual((rendered.userInfo["serverbee_target"] as? [String: Any])?["event_id"] as? String, trace["event_id"] as? String)
        }
        XCTAssertEqual(completions, 1)
    }
}

private struct NotificationEnvelopeFixture {
    let envelope: PushEnvelope
    let key: PushContentKey
    let content: PushContent
}
