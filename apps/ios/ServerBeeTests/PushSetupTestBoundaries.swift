import Foundation
import UserNotifications
@testable import ServerBee

@MainActor
final class TestPushSystem: PushSystemBoundary {
    var status: UNAuthorizationStatus = .authorized
    var permissionRequests = 0
    var registrations = 0
    var permissionHook: (() async -> Void)?
    func authorization() async -> UNAuthorizationStatus { status }
    func requestPermission() async throws -> Bool {
        permissionRequests += 1
        await permissionHook?()
        return status == .authorized
    }
    func register() { registrations += 1 }
}

@MainActor
final class TestPushRelay: PushRelayBoundary {
    var supported = true
    var attempts = 0
    var revocations = 0
    var registerHook: (() async throws -> Void)?
    func register(token: String, relayUrl: String) async throws -> RelayGrant {
        attempts += 1
        try await registerHook?()
        return RelayGrant(grantId: "fixture-grant", grantToken: "fixture-secret", keyId: "fixture-key", deviceToken: token, environment: "sandbox", expiresAt: 2_000_000_000)
    }
    func revoke(_ grant: RelayGrant, relayUrl: String) async throws { revocations += 1 }
}

enum PushSetupTestData {
    static func response(enabled: Bool = true, registered: Bool = false, revision: Int64 = 1) -> Data {
        Data("""
        {"data":{"revision":\(revision),"preferences":{"enabled":\(enabled),"alerts":true,"security":false,"task_failure":true,"task_success":false},
        "registered":\(registered),"grant_expires_at":null,"relay_url":"https://relay.test","delivery_available":false}}
        """.utf8)
    }
}
