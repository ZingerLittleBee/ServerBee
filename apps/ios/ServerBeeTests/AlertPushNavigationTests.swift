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
    private func context(user: String = "alice", server: String = "https://serverbee.test", installation: String = "alert-install") -> MobileAuthenticationContext {
        MobileAuthenticationContext(serverUrl: server, userId: user, installationId: installation, generation: UUID(),
                                    accessToken: "fixture-access", revocationToken: "fixture-proof", refreshToken: "fixture-refresh")
    }

    private func encrypted(
        status: String = "firing", alertKey: String? = nil, createdAt: Int64? = nil, lifetime: Int64 = 1800,
        authentication: MobileAuthenticationContext? = nil
    ) throws -> EncryptedAlertFixture {
        let current = authentication ?? context()
        let now = createdAt ?? Int64(Date().timeIntervalSince1970)
        let parts = ["rule-1", "server-1", "203.0.113.7", "2033-05-18T03:33:20+00:00"]
        let encoded = try JSONEncoder().encode(parts).base64EncodedString()
            .replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
        let target = alertKey ?? "v1.\(encoded)"
        let content = PushContent(kind: "alert", deploymentId: current.serverUrl, userId: current.userId,
                                  installationId: current.installationId, eventId: UUID().uuidString.lowercased(), createdAt: now, expiresAt: now + lifetime,
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
        let link = try XCTUnwrap(router.consumeTarget(context: context(), key: key))
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
            XCTAssertNil(router.consumeTarget(context: replacement, key: key))
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
            XCTAssertNil(router.consumeTarget(context: context(), key: key))
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

    func testLateColdAlertTapRetainsExactIdentityAfterWiring() throws {
        for status in ["firing", "resolved"] {
            let created = Int64(Date().timeIntervalSince1970) - 3600
            let fixture = try encrypted(status: status, createdAt: created)
            let input = UNMutableNotificationContent()
            input.userInfo = ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(fixture.envelope))]
            let presented = PushNotificationRenderer.render(input, key: fixture.key, now: created)
            XCTAssertNotNil(presented.userInfo["serverbee_target"])
            let delegate = AppDelegate()
            delegate.bufferNotification(userInfo: presented.userInfo)
            let router = PushNotificationRouter()
            delegate.pushRouter = router
            let link = try XCTUnwrap(router.consumeTarget(context: context(), key: fixture.key))
            XCTAssertEqual(link, .alertDetail(alertKey: fixture.target))
            var tab = 0
            var servers: [ServerNavigationTarget] = [.detailById("old")]
            var alerts: [ServerDeepLink] = []
            ContentView.applyDeepLink(link, selectedTab: &tab, serversPath: &servers, alertsPath: &alerts)
            XCTAssertEqual(tab, 1)
            XCTAssertEqual(alerts, [.alertDetail(alertKey: fixture.target)])
            XCTAssertEqual(servers, [.detailById("old")])
            XCTAssertNil(router.consumeTarget(context: context(), key: fixture.key))
        }
    }

    func testLateWarmAlertTapRetainsExactIdentity() throws {
        for status in ["firing", "resolved"] {
            let created = Int64(Date().timeIntervalSince1970) - 7 * 86400
            let fixture = try encrypted(status: status, createdAt: created)
            let input = UNMutableNotificationContent()
            input.userInfo = ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(fixture.envelope))]
            let presented = PushNotificationRenderer.render(input, key: fixture.key, now: created)
            let router = PushNotificationRouter()
            let delegate = AppDelegate()
            delegate.pushRouter = router
            delegate.bufferNotification(userInfo: presented.userInfo)
            XCTAssertEqual(router.consumeTarget(context: context(), key: fixture.key), .alertDetail(alertKey: fixture.target))
            XCTAssertNil(router.pendingEnvelope)
        }
    }

    func testDeliveryExpiryRemainsStrictAndLateTapAuthenticatesContent() throws {
        let created = Int64(Date().timeIntervalSince1970) - 3600
        let fixture = try encrypted(createdAt: created)
        for received in [created + 1800, created + 3600] {
            XCTAssertThrowsError(try PushEnvelopeDecoder.decrypt(fixture.envelope, key: fixture.key, now: received))
            XCTAssertEqual(try PushEnvelopeDecoder.decrypt(fixture.envelope, key: fixture.key, now: received, purpose: .notificationTap).alert?.alertKey,
                           fixture.target)
            let input = UNMutableNotificationContent()
            input.userInfo = ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(fixture.envelope))]
            let rejected = PushNotificationRenderer.render(input, key: fixture.key, now: received)
            XCTAssertEqual(rejected.body, String(localized: "Open ServerBee to view this notification."))
            XCTAssertTrue(rejected.userInfo.isEmpty)
        }
        let input = UNMutableNotificationContent()
        input.userInfo = ["serverbee_envelope": try JSONSerialization.jsonObject(with: JSONEncoder().encode(fixture.envelope))]
        let service = NotificationService()
        service.loadKey = { fixture.key }
        var completions = 0
        service.didReceive(UNNotificationRequest(identifier: "expired-alert", content: input, trigger: nil)) { rendered in
            completions += 1
            XCTAssertTrue(rendered.userInfo.isEmpty)
        }
        service.serviceExtensionTimeWillExpire()
        XCTAssertEqual(completions, 1)
        let router = PushNotificationRouter()
        for replacement in [context(user: "bob"), context(server: "https://other.test"), context(installation: "other-install"),
                            MobileAuthenticationContext(serverUrl: fixture.key.deploymentId, userId: fixture.key.userId,
                                                        installationId: fixture.key.installationId, generation: UUID(), accessToken: "new",
                                                        revocationToken: "other-login", refreshToken: "new")] {
            router.enqueue(envelope: fixture.envelope)
            XCTAssertNil(router.consumeTarget(context: replacement, key: fixture.key))
        }
        router.enqueue(envelope: fixture.envelope)
        XCTAssertNil(router.consumeTarget(context: context(), key: nil))
        let envelope = fixture.envelope
        var ciphertext = try XCTUnwrap(Data(base64Encoded: envelope.ciphertext))
        ciphertext[0] ^= 1
        let tampered = PushEnvelope(version: envelope.version, keyId: envelope.keyId, identity: envelope.identity,
                                    nonce: envelope.nonce, ciphertext: ciphertext.base64EncodedString())
        router.enqueue(envelope: tampered)
        XCTAssertNil(router.consumeTarget(context: context(), key: fixture.key))
        for lifetime in [1799, 1801] {
            let malformed = try encrypted(createdAt: created, lifetime: Int64(lifetime))
            router.enqueue(envelope: malformed.envelope)
            XCTAssertNil(router.consumeTarget(context: context(), key: malformed.key))
        }
        let oversized = PushEnvelope(version: envelope.version, keyId: envelope.keyId, identity: envelope.identity,
                                     nonce: envelope.nonce, ciphertext: String(repeating: "A", count: 2761))
        router.enqueue(envelope: oversized)
        XCTAssertNil(router.consumeTarget(context: context(), key: fixture.key))
    }

    func testLateAlertTapFetchesCurrentDetailAndClearsUnavailableTargets() async throws {
        URLProtocol.registerClass(PushLifecycleURLProtocol.self)
        let auth = AuthManager(cleanupSession: APIClient.makeCleanupSession(protocolClasses: [PushLifecycleURLProtocol.self]))
        defer {
            PushLifecycleURLProtocol.handler = nil
            PushLifecycleURLProtocol.cancelPending()
            URLProtocol.unregisterClass(PushLifecycleURLProtocol.self)
            auth.clearAuth()
            try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
        }
        auth.setServerUrl("https://serverbee.test")
        auth.handleLoginResponse(MobileTokenResponse(accessToken: "fixture-access", accessExpiresInSecs: 900,
                                                    refreshToken: "fixture-refresh", refreshExpiresInSecs: 3600, tokenType: "Bearer",
                                                    user: MobileUser(id: "alice", username: "alice", role: "member"),
                                                    revocationToken: "fixture-deletion-" + UUID().uuidString, mobileSessionId: UUID().uuidString))
        let current = try XCTUnwrap(auth.captureContext())
        let fixture = try encrypted(createdAt: Int64(Date().timeIntervalSince1970) - 3600, authentication: current)
        let router = PushNotificationRouter()
        router.enqueue(envelope: fixture.envelope)
        guard case let .alertDetail(target) = try XCTUnwrap(router.consumeTarget(context: current, key: fixture.key)) else {
            XCTFail("Late tap must enter current authenticated detail lookup")
            return
        }
        let data = try JSONSerialization.data(withJSONObject: ["data": [
            "alert_key": target, "rule_id": "rule-1", "rule_name": "Current rule", "server_id": "server-1", "server_name": "Current Server",
            "status": "resolved", "message": "Current authorized state", "trigger_count": 1,
            "first_triggered_at": "2033-05-18T03:33:20Z", "rule_enabled": true, "rule_trigger_mode": "once"
        ]])
        let viewModel = AlertDetailViewModel()
        let api = APIClient(authManager: auth)
        // Substitute only HTTP responses. Navigation does not trust the push's
        // display text as detail, and current Server denial/deletion clears it.
        for status in [200, 404, 200, 403] {
            let requested = expectation(description: "Current authenticated alert lookup: \(status)")
            PushLifecycleURLProtocol.handler = { request in
                XCTAssertEqual(request.request.url?.absoluteString, "https://serverbee.test/api/alert-events/\(target)")
                XCTAssertEqual(request.request.value(forHTTPHeaderField: "Authorization"), "Bearer fixture-access")
                requested.fulfill()
                request.respond(status, data: data)
            }
            await viewModel.fetchDetail(alertKey: target, apiClient: api)
            await fulfillment(of: [requested], timeout: 3)
            XCTAssertFalse(viewModel.isLoading)
            if status == 200 {
                XCTAssertEqual(viewModel.detail?.alertKey, target)
                XCTAssertEqual(viewModel.detail?.ruleName, "Current rule")
                XCTAssertEqual(viewModel.detail?.status, .resolved)
                XCTAssertNil(viewModel.errorMessage)
            } else {
                XCTAssertNil(viewModel.detail)
                XCTAssertEqual(viewModel.errorMessage, String(localized: "Alert not found"))
            }
        }
    }

}
