import Foundation

struct MobileLoginRequest: Codable, Sendable {
    let username: String
    let password: String
    let installationId: String
    var totpCode: String?

    enum CodingKeys: String, CodingKey {
        case username
        case password
        case installationId = "installation_id"
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

    enum CodingKeys: String, CodingKey {
        case accessToken = "access_token"
        case accessExpiresInSecs = "access_expires_in_secs"
        case refreshToken = "refresh_token"
        case refreshExpiresInSecs = "refresh_expires_in_secs"
        case tokenType = "token_type"
        case user
    }
}

struct MobileUser: Codable, Hashable, Sendable {
    let id: String
    let username: String
    let role: String
}

struct MobileRefreshRequest: Codable, Sendable {
    let refreshToken: String
    let installationId: String

    enum CodingKeys: String, CodingKey {
        case refreshToken = "refresh_token"
        case installationId = "installation_id"
    }
}

struct MobileLogoutRequest: Codable, Sendable {
    let refreshToken: String
    let installationId: String

    enum CodingKeys: String, CodingKey {
        case refreshToken = "refresh_token"
        case installationId = "installation_id"
    }
}
