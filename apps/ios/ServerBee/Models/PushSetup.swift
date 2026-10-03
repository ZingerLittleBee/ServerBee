import Foundation

struct PushPreferences: Codable, Sendable, Equatable {
    var enabled = false
    var alerts = false
    var security = false
    var taskFailure = false
    var taskSuccess = false

    enum CodingKeys: String, CodingKey {
        case enabled, alerts, security
        case taskFailure = "task_failure"
        case taskSuccess = "task_success"
    }
}

struct PushSetup: Decodable, Sendable {
    let revision: Int64
    let preferences: PushPreferences
    let securityAllowed: Bool
    var registered: Bool
    let grantExpiresAt: String?
    let relayUrl: String
    let deliveryAvailable: Bool

    enum CodingKeys: String, CodingKey {
        case revision, preferences, registered
        case securityAllowed = "security_allowed"
        case grantExpiresAt = "grant_expires_at"
        case relayUrl = "relay_url"
        case deliveryAvailable = "delivery_available"
    }
}

struct PushPreferencesRequest: Encodable, Sendable {
    let expectedRevision: Int64
    let preferences: PushPreferences
    enum CodingKeys: String, CodingKey {
        case expectedRevision = "expected_revision"
        case preferences
    }
}

struct RelayGrant: Codable, Sendable {
    let grantId: String
    let grantToken: String
    let keyId: String
    let deviceToken: String
    let environment: String
    let expiresAt: Int64
    enum CodingKeys: String, CodingKey {
        case grantId = "grant_id"
        case grantToken = "grant_token"
        case keyId = "key_id"
        case deviceToken = "device_token"
        case environment
        case expiresAt = "expires_at"
    }
}

struct VerifiedPushRequest: Encodable, Sendable {
    let expectedRevision: Int64
    let deviceToken: String
    let environment: String
    let keyId: String
    let grantId: String
    let grantToken: String
    let contentKeyId: String
    let contentKey: String
    let deploymentId: String
    enum CodingKeys: String, CodingKey {
        case expectedRevision = "expected_revision"
        case deviceToken = "device_token"
        case environment
        case keyId = "key_id"
        case grantId = "grant_id"
        case grantToken = "grant_token"
        case contentKeyId = "content_key_id"
        case contentKey = "content_key"
        case deploymentId = "deployment_id"
    }
}

struct TestPushRequest: Encodable, Sendable {
    let expectedRevision: Int64
    enum CodingKeys: String, CodingKey { case expectedRevision = "expected_revision" }
}

struct TestPushResponse: Decodable, Sendable {
    let eventId: String
    let outcome: String
    let reason: String
    let presentation: String
    enum CodingKeys: String, CodingKey {
        case outcome, reason, presentation
        case eventId = "event_id"
    }
}
