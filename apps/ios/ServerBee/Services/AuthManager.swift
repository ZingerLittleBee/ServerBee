import Foundation
import Observation

@MainActor
@Observable
final class AuthManager: Sendable {
    private(set) var isAuthenticated = false
    private(set) var user: MobileUser?
    private(set) var serverUrl: String?

    private static let accessTokenKey = "access_token"
    private static let refreshTokenKey = "refresh_token"
    private static let serverUrlKey = "server_url"
    private static let userKey = "user"

    init() {
        restoreSession()
    }

    var accessToken: String? {
        KeychainService.loadString(key: Self.accessTokenKey)
    }

    var refreshToken: String? {
        KeychainService.loadString(key: Self.refreshTokenKey)
    }

    func setAuth(
        accessToken: String,
        refreshToken: String,
        user: MobileUser,
        serverUrl: String
    ) {
        persistTokens(accessToken: accessToken, refreshToken: refreshToken)
        KeychainService.saveString(key: Self.serverUrlKey, value: serverUrl)
        persistUser(user)

        self.user = user
        self.serverUrl = serverUrl
        self.isAuthenticated = true
    }

    func updateTokens(accessToken: String, refreshToken: String, user: MobileUser) {
        persistTokens(accessToken: accessToken, refreshToken: refreshToken)
        persistUser(user)

        self.user = user
    }

    func clearAuth() {
        KeychainService.delete(key: Self.accessTokenKey)
        KeychainService.delete(key: Self.refreshTokenKey)
        KeychainService.delete(key: Self.serverUrlKey)
        KeychainService.delete(key: Self.userKey)

        self.isAuthenticated = false
        self.user = nil
        self.serverUrl = nil
    }

    // MARK: - Private

    private func persistTokens(accessToken: String, refreshToken: String) {
        KeychainService.saveString(key: Self.accessTokenKey, value: accessToken)
        KeychainService.saveString(key: Self.refreshTokenKey, value: refreshToken)
    }

    private func persistUser(_ user: MobileUser) {
        if let data = try? JSONEncoder().encode(user) {
            KeychainService.save(key: Self.userKey, data: data)
        }
    }

    private func restoreSession() {
        guard let _ = KeychainService.loadString(key: Self.accessTokenKey),
              let _ = KeychainService.loadString(key: Self.refreshTokenKey),
              let serverUrl = KeychainService.loadString(key: Self.serverUrlKey),
              let userData = KeychainService.load(key: Self.userKey),
              let user = try? JSONDecoder().decode(MobileUser.self, from: userData)
        else {
            return
        }

        self.user = user
        self.serverUrl = serverUrl
        self.isAuthenticated = true
    }
}
