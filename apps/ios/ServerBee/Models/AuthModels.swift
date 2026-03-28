import Foundation

struct MobileLoginRequest: Encodable, Sendable {
    let username: String
    let password: String
    let installationId: String
    var totpCode: String?
}

struct MobileTokenResponse: Decodable, Sendable {
    let accessToken: String
    let refreshToken: String
    let accessExpiresInSecs: Int
    let refreshExpiresInSecs: Int
    let tokenType: String
    let user: MobileUser
}

struct MobileUser: Codable, Sendable {
    let id: String
    let username: String
    let role: String
}

struct MobileRefreshRequest: Encodable, Sendable {
    let installationId: String
    let refreshToken: String
}

struct ApiResponse<T: Decodable & Sendable>: Decodable, Sendable {
    let data: T
}
