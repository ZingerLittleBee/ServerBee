import Foundation
import SwiftUI

/// Manages authentication state for the mobile app.
@Observable
final class AuthManager: @unchecked Sendable {
    // MARK: - Published State

    var isLoading = true
    var isAuthenticated = false
    var user: MobileUser?
    var serverUrl: String?

    // MARK: - Lifecycle

    func initialize() async {
        isLoading = true
        defer { isLoading = false }

        serverUrl = KeychainService.loadString(for: KeychainService.serverUrlKey)

        guard KeychainService.loadString(for: KeychainService.accessTokenKey) != nil,
              let _: MobileUser = KeychainService.loadCodable(for: KeychainService.userKey)
        else {
            return
        }

        if let refreshToken = KeychainService.loadString(for: KeychainService.refreshTokenKey) {
            do {
                let response = try await refreshTokens(refreshToken: refreshToken)
                await handleLoginResponse(response)
            } catch {
                await clearAuth()
            }
        }
    }

    // MARK: - Server URL

    func setServerUrl(_ url: String) {
        serverUrl = url
        try? KeychainService.saveString(url, for: KeychainService.serverUrlKey)
    }

    // MARK: - Login Handling

    @MainActor
    func handleLoginResponse(_ response: MobileTokenResponse) {
        try? KeychainService.saveString(response.accessToken, for: KeychainService.accessTokenKey)
        try? KeychainService.saveString(response.refreshToken, for: KeychainService.refreshTokenKey)
        try? KeychainService.saveCodable(response.user, for: KeychainService.userKey)
        user = response.user
        isAuthenticated = true
    }

    // MARK: - Token Access

    func getAccessToken() -> String? {
        KeychainService.loadString(for: KeychainService.accessTokenKey)
    }

    // MARK: - Logout

    @MainActor
    func clearAuth() {
        KeychainService.delete(for: KeychainService.accessTokenKey)
        KeychainService.delete(for: KeychainService.refreshTokenKey)
        KeychainService.delete(for: KeychainService.userKey)
        user = nil
        isAuthenticated = false
    }

    // MARK: - Token Refresh (private)

    private func refreshTokens(refreshToken: String) async throws -> MobileTokenResponse {
        guard let serverUrl else {
            throw AuthError.noServerUrl
        }

        guard let url = URL(string: "\(serverUrl)/api/mobile/auth/refresh") else {
            throw AuthError.noServerUrl
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let body = MobileRefreshRequest(
            refreshToken: refreshToken,
            installationId: InstallationID.getOrCreate()
        )
        request.httpBody = try JSONEncoder.snakeCase.encode(body)

        let (data, response) = try await URLSession.shared.data(for: request)

        guard let httpResponse = response as? HTTPURLResponse,
              httpResponse.statusCode == 200
        else {
            throw AuthError.refreshFailed
        }

        let apiResponse = try JSONDecoder.snakeCase.decode(
            ApiResponse<MobileTokenResponse>.self,
            from: data
        )
        return apiResponse.data
    }
}

// MARK: - Auth Errors

enum AuthError: Error, LocalizedError {
    case noServerUrl
    case refreshFailed
    case invalidCredentials
    case twoFactorRequired
    case tooManyAttempts
    case networkError(Error)

    var errorDescription: String? {
        switch self {
        case .noServerUrl:
            return "No server URL configured"
        case .refreshFailed:
            return "Token refresh failed — please log in again"
        case .invalidCredentials:
            return "Invalid username or password"
        case .twoFactorRequired:
            return "Two-factor authentication is required"
        case .tooManyAttempts:
            return "Too many login attempts — please try again later"
        case .networkError(let error):
            return "Network error: \(error.localizedDescription)"
        }
    }
}
