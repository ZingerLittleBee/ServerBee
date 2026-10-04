import Foundation

/// Display-only identity. Original credentials remain in private auth storage.
struct SessionRecoveryIdentity: Equatable {
    let loginId: UUID
    let serverUrl: String
    let username: String
    let selectedSessionId: String?

    init(_ authentication: SavedMobileAuthentication) {
        loginId = authentication.loginId
        serverUrl = authentication.serverUrl
        username = authentication.user.username
        selectedSessionId = authentication.recoveryTargetSessionId
    }
}

enum SessionRecoveryAuthorization: Sendable {
    case password(username: String, password: String, totpCode: String?)
    case pairingCode(String)
    case recoveryToken(String)

    struct PasswordCredentials {
        let username: String
        let password: String
        let totpCode: String?
    }

    var passwordCredentials: PasswordCredentials? {
        if case let .password(username, password, totpCode) = self {
            return PasswordCredentials(username: username, password: password, totpCode: totpCode)
        }
        return nil
    }
}

struct MobileSessionRecoveryRequest: Encodable, Sendable {
    let username: String?
    let password: String?
    let totpCode: String?
    let pairingCode: String?
    let recoveryToken: String?
    let expectedUserId: String
    let installationId: String
    let expectedSessionId: String?
    let accessToken: String
    let refreshToken: String
    let revocationToken: String?

    init(original: SavedMobileAuthentication, authorization: SessionRecoveryAuthorization) {
        username = authorization.passwordCredentials?.username
        password = authorization.passwordCredentials?.password
        totpCode = authorization.passwordCredentials?.totpCode
        if case let .pairingCode(code) = authorization { pairingCode = code } else { pairingCode = nil }
        if case let .recoveryToken(token) = authorization { recoveryToken = token } else { recoveryToken = nil }
        expectedUserId = original.user.id
        installationId = original.installationId
        expectedSessionId = original.recoveryTargetSessionId
        accessToken = original.accessToken
        refreshToken = original.refreshToken
        revocationToken = original.revocationToken
    }

    enum CodingKeys: String, CodingKey {
        case username, password
        case totpCode = "totp_code"
        case pairingCode = "pairing_code"
        case recoveryToken = "recovery_token"
        case expectedUserId = "expected_user_id"
        case installationId = "installation_id"
        case expectedSessionId = "expected_session_id"
        case accessToken = "access_token"
        case refreshToken = "refresh_token"
        case revocationToken = "revocation_token"
    }
}

struct MobileSessionRecoveryResponse: Decodable, Sendable {
    enum Outcome: String, Decodable {
        case ok
        case alreadyAbsent = "already_absent"
        case selectionRequired = "selection_required"
    }
    let outcome: Outcome
    let userId: String
    let installationId: String
    let mobileSessionId: String?
    let candidates: [MobileRecoveryCandidate]?
    let recoveryToken: String?

    enum CodingKeys: String, CodingKey {
        case outcome
        case userId = "user_id"
        case installationId = "installation_id"
        case mobileSessionId = "mobile_session_id"
        case candidates
        case recoveryToken = "recovery_token"
    }

    func isValid(for original: SavedMobileAuthentication) -> Bool {
        guard userId == original.user.id, installationId == original.installationId else { return false }
        if outcome == .selectionRequired {
            guard original.recoveryTargetSessionId == nil, mobileSessionId == nil,
                  let candidates, !candidates.isEmpty, candidates.count <= 64 else { return false }
            let ids = candidates.compactMap { UUID(uuidString: $0.mobileSessionId) }
            return ids.count == candidates.count && Set(ids).count == ids.count
        }
        guard candidates?.isEmpty != false else { return false }
        if let known = original.recoveryTargetSessionId {
            guard let expected = UUID(uuidString: known), let mobileSessionId,
                  UUID(uuidString: mobileSessionId) == expected else { return false }
            return true
        }
        switch outcome {
        case .alreadyAbsent: return mobileSessionId == nil
        case .ok: return mobileSessionId.flatMap(UUID.init(uuidString:)) != nil
        case .selectionRequired: return false
        }
    }
}

struct MobileRecoveryCandidate: Decodable, Sendable, Identifiable, Equatable {
    let mobileSessionId: String
    let deviceName: String
    let createdAt: String
    let lastUsedAt: String
    var id: String { mobileSessionId }

    enum CodingKeys: String, CodingKey {
        case mobileSessionId = "mobile_session_id"
        case deviceName = "device_name"
        case createdAt = "created_at"
        case lastUsedAt = "last_used_at"
    }
}
