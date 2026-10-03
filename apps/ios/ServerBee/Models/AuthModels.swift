import Foundation

struct MobileLoginRequest: Codable, Sendable {
    let username: String
    let password: String
    let installationId: String
    let deviceName: String
    var totpCode: String?

    enum CodingKeys: String, CodingKey {
        case username
        case password
        case installationId = "installation_id"
        case deviceName = "device_name"
        case totpCode = "totp_code"
    }
}

struct MobileTokenResponse: Codable, Sendable {
    let accessToken: String
    let accessExpiresInSecs: Int
    let refreshToken: String
    let refreshExpiresInSecs: Int
    let tokenType: String
    let user: MobileUser
    var revocationToken: String?
    var mobileSessionId: String?

    enum CodingKeys: String, CodingKey {
        case accessToken = "access_token"
        case accessExpiresInSecs = "access_expires_in_secs"
        case refreshToken = "refresh_token"
        case refreshExpiresInSecs = "refresh_expires_in_secs"
        case tokenType = "token_type"
        case user
        case revocationToken = "revocation_token"
        case mobileSessionId = "mobile_session_id"
    }
}

struct MobileUser: Codable, Hashable, Sendable {
    let id: String
    let username: String
    let role: String

    enum CodingKeys: String, CodingKey {
        case id
        case username
        case role
    }
}

/// Current HTTP authorization, independent of the cached mobile-login user.
struct CurrentUserResponse: Decodable, Sendable {
    let userId: String
    let username: String
    let role: String
    let mustChangePassword: Bool

    enum CodingKeys: String, CodingKey {
        case userId = "user_id"
        case username, role
        case mustChangePassword = "must_change_password"
    }
}

struct MobileRefreshRequest: Codable, Sendable {
    let refreshToken: String
    let installationId: String
    var revocationProof: String?

    enum CodingKeys: String, CodingKey {
        case refreshToken = "refresh_token"
        case installationId = "installation_id"
        case revocationProof = "revocation_proof"
    }
}

struct MobileRevokeRequest: Codable, Sendable {
    let installationId: String
    let revocationToken: String
    var expectedSessionId: String?

    enum CodingKeys: String, CodingKey {
        case installationId = "installation_id"
        case revocationToken = "revocation_token"
        case expectedSessionId = "expected_session_id"
    }
}
