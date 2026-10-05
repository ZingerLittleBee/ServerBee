import Foundation
import Observation

@MainActor
@Observable
final class SettingsViewModel {
    var showLogoutConfirmation = false
    var isLoggingOut = false

    /// Close the socket and unregister before the shared durable logout path.
    /// A failed remote revoke remains pending; migration/storage failures are surfaced.
    func logout(
        authManager: AuthManager,
        unregisterPush: @escaping @MainActor (MobileAuthenticationContext?) async -> Void,
        closeWebSocket: @escaping @MainActor () async -> Void
    ) async {
        let generation = authManager.authenticationGeneration
        isLoggingOut = true
        defer { isLoggingOut = false }
        if let context = authManager.captureContext() {
            await authManager.endSession(context: context) {
                await closeWebSocket()
                await unregisterPush(context)
            }
        } else {
            await closeWebSocket()
            await unregisterPush(nil)
            if authManager.authenticationGeneration == generation, authManager.sessionRecovery == nil { authManager.clearAuth() }
        }
    }
}
