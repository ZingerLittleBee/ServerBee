import Foundation

struct MobileUser: Codable, Sendable, Equatable {
    let userId: String
    let username: String
    let role: String
    let mustChangePassword: Bool
}

struct LoginRequest: Codable, Sendable {
    let username: String
    let password: String
    let totpCode: String?
    let installationId: String
}

struct LoginResponse: Codable, Sendable {
    let accessToken: String
    let accessExpiresInSecs: Int
    let refreshToken: String
    let refreshExpiresInSecs: Int
    let tokenType: String
    let user: MobileUser
}

struct RefreshRequest: Codable, Sendable {
    let refreshToken: String
    let installationId: String
}

struct RefreshResponse: Codable, Sendable {
    let accessToken: String
    let accessExpiresInSecs: Int
    let refreshToken: String
    let refreshExpiresInSecs: Int
    let tokenType: String
    let user: MobileUser
}

struct LogoutRequest: Codable, Sendable {
    let refreshToken: String
    let installationId: String
}
