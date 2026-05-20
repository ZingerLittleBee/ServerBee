import Foundation

struct MobileLoginRequest: Codable, Sendable {
    let username: String
    let password: String
    let installationId: String
    let deviceName: String
    var totpCode: String?
}

struct MobileTokenResponse: Codable, Sendable {
    let accessToken: String
    let accessExpiresInSecs: Int
    let refreshToken: String
    let refreshExpiresInSecs: Int
    let tokenType: String
    let user: MobileUser
}

struct MobileUser: Codable, Hashable, Sendable {
    let id: String
    let username: String
    let role: String
}

struct MobileRefreshRequest: Codable, Sendable {
    let refreshToken: String
    let installationId: String
}
