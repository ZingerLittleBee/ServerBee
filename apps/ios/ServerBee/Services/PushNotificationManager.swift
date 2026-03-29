import Foundation
import UserNotifications
import UIKit

@Observable
final class PushNotificationManager: NSObject, @unchecked Sendable {
    var permissionGranted = false
    var deviceToken: String?

    private var apiClient: APIClient?

    func configure(apiClient: APIClient) {
        self.apiClient = apiClient
    }

    /// Request notification permission and register for remote notifications.
    @MainActor
    func requestPermission() async {
        do {
            let granted = try await UNUserNotificationCenter.current()
                .requestAuthorization(options: [.alert, .badge, .sound])
            permissionGranted = granted
            if granted {
                UIApplication.shared.registerForRemoteNotifications()
            }
        } catch {
            print("[Push] Permission request failed: \(error)")
        }
    }

    /// Called when APNs assigns a device token.
    func didRegisterForRemoteNotifications(deviceToken data: Data) {
        let token = data.map { String(format: "%02x", $0) }.joined()
        self.deviceToken = token
        Task {
            await registerTokenWithServer(token)
        }
    }

    /// Called when APNs registration fails.
    func didFailToRegisterForRemoteNotifications(error: Error) {
        print("[Push] Registration failed: \(error)")
    }

    /// Upload device token to server.
    private func registerTokenWithServer(_ token: String) async {
        guard let apiClient else { return }
        do {
            try await apiClient.postVoid("/api/mobile/push/register", body: ["device_token": token])
        } catch {
            print("[Push] Failed to register token with server: \(error)")
        }
    }

    /// Unregister device token from server (called on logout).
    func unregister() async {
        guard let apiClient else { return }
        try? await apiClient.postVoid("/api/mobile/push/unregister")
        deviceToken = nil
    }

    /// Handle notification tap — extract server_id for deep linking.
    func handleNotificationResponse(_ response: UNNotificationResponse) -> String? {
        let userInfo = response.notification.request.content.userInfo
        return userInfo["server_id"] as? String
    }
}
