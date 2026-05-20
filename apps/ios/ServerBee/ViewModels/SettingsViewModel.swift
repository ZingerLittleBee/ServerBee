import Foundation
import Observation

@MainActor
@Observable
final class SettingsViewModel {
    var showLogoutConfirmation = false
    var isLoggingOut = false

    /// Logs the user out:
    /// 1. Closes the live WebSocket so the server doesn't see noisy reconnect
    ///    attempts with a revoked access token during the brief unregister
    ///    window. Injected as a closure to avoid tight coupling to the
    ///    `WebSocketClient` actor type.
    /// 2. Unregisters the device push token (best-effort; failure is logged but
    ///    does not block logout — the next register call rebinds the token to
    ///    the new user).
    /// 3. Tells the server to revoke this mobile session.
    /// 4. Clears local auth state last so the UI returns to LoginView.
    func logout(
        authManager: AuthManager,
        apiClient: APIClient,
        pushManager: any PushNotificationManaging,
        closeWebSocket: @MainActor () async -> Void
    ) async {
        isLoggingOut = true
        defer { isLoggingOut = false }

        // 1. Close the live WebSocket FIRST so the actor task tree stops
        //    attempting reconnects with the soon-to-be-revoked token.
        await closeWebSocket()

        // 2. Unregister push device token. Must happen BEFORE clearAuth so the
        //    bearer token is still valid for the request, and BEFORE the server
        //    logout so we do not leak a stale device-token binding to this user.
        await pushManager.unregister()

        // 3. Best-effort server-side logout (Bearer token provides identity).
        try? await apiClient.postVoid("/api/mobile/auth/logout")

        // 4. Clear local auth last.
        authManager.clearAuth()
    }
}
