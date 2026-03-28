import Foundation

@Observable
@MainActor
final class AuthManager {
    private(set) var isAuthenticated = false
    private(set) var isLoading = true
    private(set) var user: MobileUser?
    private(set) var serverUrl: String?

    private enum Keys {
        static let accessToken = "serverbee_access_token"
        static let refreshToken = "serverbee_refresh_token"
        static let user = "serverbee_user"
        static let serverUrl = "serverbee_server_url"
    }

    func initialize() async {
        let storedUrl = KeychainService.load(key: Keys.serverUrl)
        let storedAccessToken = KeychainService.load(key: Keys.accessToken)
        let storedUserJson = KeychainService.load(key: Keys.user)

        guard let url = storedUrl,
              let accessToken = storedAccessToken,
              KeychainService.load(key: Keys.refreshToken) != nil
        else {
            isLoading = false
            isAuthenticated = false
            return
        }

        serverUrl = url

        do {
            var request = URLRequest(url: URL(string: "\(url)/api/auth/me")!)
            request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")

            let (data, response) = try await URLSession.shared.data(for: request)
            let httpResponse = response as! HTTPURLResponse

            if httpResponse.statusCode == 200 {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let meResponse = try decoder.decode(ApiResponse<MeResponse>.self, from: data)
                let fetchedUser = MobileUser(
                    id: meResponse.data.userId,
                    username: meResponse.data.username,
                    role: meResponse.data.role
                )
                user = fetchedUser
                saveUser(fetchedUser)
                isAuthenticated = true
            } else {
                clearTokens()
                fallBackToCachedUser(storedUserJson)
            }
        } catch {
            // Network error -- use cached user as fallback
            fallBackToCachedUser(storedUserJson)
        }

        isLoading = false
    }

    func setServerUrl(_ url: String) {
        let normalized = url.hasSuffix("/") ? String(url.dropLast()) : url
        serverUrl = normalized
        KeychainService.save(key: Keys.serverUrl, value: normalized)
    }

    func handleLoginResponse(_ response: MobileTokenResponse) {
        KeychainService.save(key: Keys.accessToken, value: response.accessToken)
        KeychainService.save(key: Keys.refreshToken, value: response.refreshToken)
        saveUser(response.user)
        user = response.user
        isAuthenticated = true
    }

    func getAccessToken() -> String? {
        KeychainService.load(key: Keys.accessToken)
    }

    func getRefreshToken() -> String? {
        KeychainService.load(key: Keys.refreshToken)
    }

    func clearAuth() {
        clearTokens()
        KeychainService.delete(key: Keys.user)
        KeychainService.delete(key: Keys.serverUrl)
        isAuthenticated = false
        user = nil
        serverUrl = nil
    }

    // MARK: - Private

    private func fallBackToCachedUser(_ storedUserJson: String?) {
        guard let userJson = storedUserJson else { return }
        user = decodeUser(from: userJson)
        isAuthenticated = user != nil
    }

    private func clearTokens() {
        KeychainService.delete(key: Keys.accessToken)
        KeychainService.delete(key: Keys.refreshToken)
    }

    private func saveUser(_ user: MobileUser) {
        if let data = try? JSONEncoder().encode(user),
           let json = String(data: data, encoding: .utf8)
        {
            KeychainService.save(key: Keys.user, value: json)
        }
    }

    private func decodeUser(from json: String) -> MobileUser? {
        guard let data = json.data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(MobileUser.self, from: data)
    }
}

private struct MeResponse: Decodable, Sendable {
    let userId: String
    let username: String
    let role: String
}
