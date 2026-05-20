import Foundation
import Observation

@MainActor
@Observable
final class SettingsViewModel {
    var showLogoutConfirmation = false
    var isLoggingOut = false

    /// Logs the user out:
    /// 1. Unregisters the device push token (best-effort; failure is logged but
    ///    does not block logout — the next register call rebinds the token to
    ///    the new user).
    /// 2. Tells the server to revoke this mobile session.
    /// 3. Clears local auth state last so the UI returns to LoginView.
    func logout(
        authManager: AuthManager,
        apiClient: APIClient,
        pushManager: any PushNotificationManaging
    ) async {
        isLoggingOut = true
        defer { isLoggingOut = false }

        // 1. Unregister push device token. Must happen BEFORE clearAuth so the
        //    bearer token is still valid for the request, and BEFORE the server
        //    logout so we do not leak a stale device-token binding to this user.
        await pushManager.unregister()

        // 2. Best-effort server-side logout (Bearer token provides identity).
        try? await apiClient.postVoid("/api/mobile/auth/logout")

        // 3. Clear local auth last.
        authManager.clearAuth()
    }
}
