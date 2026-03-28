import Foundation

// MARK: - User

struct MobileUser: Codable, Sendable {
    let id: Int
    let username: String
    let role: String
}

// MARK: - Auth Requests

struct MobileLoginRequest: Codable, Sendable {
    let username: String
    let password: String
    let installationId: String
    let deviceName: String
    let deviceModel: String
    let osVersion: String

    enum CodingKeys: String, CodingKey {
        case username
        case password
        case installationId = "installation_id"
        case deviceName = "device_name"
        case deviceModel = "device_model"
        case osVersion = "os_version"
    }
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

    enum CodingKeys: String, CodingKey {
        case refreshToken = "refresh_token"
    }
}

// MARK: - Auth Responses

struct MobileTokenResponse: Codable, Sendable {
    let accessToken: String
    let refreshToken: String
    let expiresIn: Int
    let user: MobileUser

    enum CodingKeys: String, CodingKey {
        case accessToken = "access_token"
        case refreshToken = "refresh_token"
        case expiresIn = "expires_in"
        case user
    }
}
