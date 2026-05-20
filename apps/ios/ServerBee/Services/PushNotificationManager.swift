import Foundation
import UIKit
import UserNotifications

/// Protocol abstraction so tests can inject a spy.
@MainActor
protocol PushNotificationManaging: AnyObject {
    var permissionGranted: Bool { get }
    var deviceToken: String? { get }

    func configure(apiClient: APIClient)
    func requestPermission() async

    nonisolated func didRegisterForRemoteNotifications(deviceToken data: Data)
    nonisolated func didFailToRegisterForRemoteNotifications(error: Error)

    /// Parse a push payload and return a deep link (or nil if not actionable).
    nonisolated func handleNotificationResponse(_ response: UNNotificationResponse) -> ServerDeepLink?

    /// Unregister the device token from the server. Must NOT throw — failures
    /// are logged. Local auth must still clear even if the server call fails.
    func unregister() async
}

@Observable
final class PushNotificationManager: NSObject, PushNotificationManaging, @unchecked Sendable {
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
            AppLog.push.error("Permission request failed: \(String(describing: error), privacy: .public)")
        }
    }

    /// Called when APNs assigns a device token.
    nonisolated func didRegisterForRemoteNotifications(deviceToken data: Data) {
        let token = data.map { String(format: "%02x", $0) }.joined()
        Task { @MainActor in
            self.deviceToken = token
            await self.registerTokenWithServer(token)
        }
    }

    /// Called when APNs registration fails.
    nonisolated func didFailToRegisterForRemoteNotifications(error: Error) {
        AppLog.push.error("Registration failed: \(String(describing: error), privacy: .public)")
    }

    /// Upload device token to server.
    @MainActor
    private func registerTokenWithServer(_ token: String) async {
        guard let apiClient else { return }
        do {
            try await apiClient.postVoid("/api/mobile/push/register", body: ["device_token": token])
        } catch {
            AppLog.push.error("Failed to register token with server: \(String(describing: error), privacy: .public)")
        }
    }

    /// Unregister device token from server (called on logout).
    /// Errors are swallowed — the device token will be re-bound on next register.
    @MainActor
    func unregister() async {
        guard let apiClient else {
            deviceToken = nil
            return
        }
        do {
            try await apiClient.postVoid("/api/mobile/push/unregister")
        } catch {
            AppLog.push.error("Failed to unregister token with server: \(String(describing: error), privacy: .public)")
        }
        deviceToken = nil
    }

    /// Parse a notification tap into a deep link.
    /// Backend payload (see `crates/server/src/service/apns.rs`) attaches
    /// `server_id` and optionally `rule_id` as APNs custom data.
    nonisolated func handleNotificationResponse(_ response: UNNotificationResponse) -> ServerDeepLink? {
        let userInfo = response.notification.request.content.userInfo
        if let serverId = userInfo["server_id"] as? String, !serverId.isEmpty {
            return .serverDetail(serverId: serverId)
        }
        if let ruleId = userInfo["rule_id"] as? String, !ruleId.isEmpty {
            return .alertDetail(alertKey: ruleId)
        }
        return nil
    }
}
