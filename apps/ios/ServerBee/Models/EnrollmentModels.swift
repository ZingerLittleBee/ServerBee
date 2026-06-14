import Foundation

/// A freshly-minted enrollment code. The plaintext `code` is returned ONLY at
/// mint time (create / recover / regenerate) and can never be fetched again.
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
    let name: String
    var groupId: String?

    enum CodingKeys: String, CodingKey {
        case name
        case groupId = "group_id"
    }
}

/// `POST /api/servers` response: the new (pending) server id + its first code.
struct CreateServerResponse: Decodable, Sendable {
    let serverId: String
    let enrollment: EnrollmentIssue

    enum CodingKeys: String, CodingKey {
        case serverId = "server_id"
        case enrollment
    }
}

/// Body for `POST /api/servers/{id}/recover`. When `revokeImmediately` is true
/// the existing agent token is cleared and the connected agent kicked.
struct RecoverRequest: Encodable, Sendable {
    let revokeImmediately: Bool

    enum CodingKeys: String, CodingKey {
        case revokeImmediately = "revoke_immediately"
    }
}

/// Body for `POST /api/servers/{id}/regenerate-code`. Omit `expectedEnrollmentId`
/// for last-writer-wins (mobile default).
struct RegenerateCodeRequest: Encodable, Sendable {
    var expectedEnrollmentId: String?

    enum CodingKeys: String, CodingKey {
        case expectedEnrollmentId = "expected_enrollment_id"
    }
}

/// Both recover and regenerate return `{ enrollment }`.
struct EnrollmentOnlyResponse: Decodable, Sendable {
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
