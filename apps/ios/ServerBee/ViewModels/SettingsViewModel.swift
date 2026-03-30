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

        // Best effort: POST logout to server (Bearer token provides identity)
        try? await apiClient.postVoid("/api/mobile/auth/logout")

        await authManager.clearAuth()
    }
}
