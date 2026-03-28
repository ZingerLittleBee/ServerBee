import Foundation
import Observation

@MainActor
@Observable
final class SettingsViewModel {
    var showLogoutConfirmation = false
    var isLoggingOut = false

    func logout(authManager: AuthManager, apiClient: APIClient) async {
        isLoggingOut = true
        defer { isLoggingOut = false }

        // Best effort: POST logout to server, don't fail on error
        if let refreshToken = KeychainService.loadString(for: KeychainService.refreshTokenKey) {
            let installationId = InstallationID.getOrCreate()
            let request = MobileLogoutRequest(
                refreshToken: refreshToken,
                installationId: installationId
            )
            try? await apiClient.postVoid("/api/mobile/auth/logout", body: request)
        }

        authManager.clearAuth()
    }
}
