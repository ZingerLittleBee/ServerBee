import Foundation

/// A freshly issued enrollment code. The plaintext `code` is returned only at
/// issue time and can never be fetched again.
struct EnrollmentIssue: Decodable, Sendable {
    let id: String
    let code: String
    let codePrefix: String
    let expiresAt: String

    enum CodingKeys: String, CodingKey {
        case id, code
        case codePrefix = "code_prefix"
        case expiresAt = "expires_at"
    }
}

/// Request body for `POST /api/servers` (create a pending server). Mobile keeps
/// this minimal — a name; the agent reports the rest after it connects.
struct CreateServerRequest: Encodable, Sendable {
    let onboardingRequestId: String
    let name: String
    var groupId: String?

    enum CodingKeys: String, CodingKey {
        case name
        case onboardingRequestId = "onboarding_request_id"
        case groupId = "group_id"
    }
}

/// `POST /api/servers` may replay an earlier request. Replay never recovers the
/// original plaintext code and only returns current offer metadata.
struct CreateServerResponse: Decodable, Sendable {
    let serverId: String
    let replayed: Bool
    let enrollment: EnrollmentIssue?
    let outstandingOffer: OutstandingOffer?

    enum CodingKeys: String, CodingKey {
        case serverId = "server_id"
        case replayed, enrollment
        case outstandingOffer = "outstanding_offer"
    }
}

enum ReenrollmentMode: String, Encodable, Sendable {
    case graceful
    case emergency
}

struct ReenrollmentRequest: Encodable, Sendable {
    let mode: ReenrollmentMode
}

struct IssueOfferRequest: Encodable, Sendable {
    enum CodingKeys: CodingKey {}
}

struct RevokeOfferResponse: Decodable, Sendable {
    let offerId: String
    let alreadyRevoked: Bool

    enum CodingKeys: String, CodingKey {
        case offerId = "offer_id"
        case alreadyRevoked = "already_revoked"
    }
}

struct RevokeAuthorityResponse: Decodable, Sendable {
    let serverId: String
    let changed: Bool

    enum CodingKeys: String, CodingKey {
        case serverId = "server_id"
        case changed
    }
}

struct EnrollmentOfferResponse: Decodable, Sendable {
    let enrollment: EnrollmentIssue
}

/// Body for `POST /api/servers/{id}/upgrade`. The server validates `version`
/// as strict SemVer (it strips an optional leading `v`), so free-form values
/// like "latest" are rejected — always send a concrete release version.
struct UpgradeRequest: Encodable, Sendable {
    let version: String
}

/// `GET /api/agent/latest-version` — the newest released agent version known to
/// the server (from its configured release source). Any field may be nil.
struct LatestAgentVersion: Decodable, Sendable {
    let version: String?
    let releasedAt: String?
    let error: String?

    enum CodingKeys: String, CodingKey {
        case version, error
        case releasedAt = "released_at"
    }
}
