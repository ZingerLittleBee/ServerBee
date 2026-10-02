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
    func unregister(context: MobileAuthenticationContext?) async
}

extension PushNotificationManaging {
    func unregister(context: MobileAuthenticationContext?) async {
        await unregister()
    }
}

@MainActor
@Observable
final class PushNotificationManager: NSObject, PushNotificationManaging {
    var permissionGranted = false
    var deviceToken: String?

    private var apiClient: APIClient?
    private var context: MobileAuthenticationContext?
    private var acceptingRegistrations = false
    private var uploads: [UUID: (generation: UUID, task: Task<Void, Never>)] = [:]

    func configure(apiClient: APIClient) {
        self.apiClient = apiClient
        context = apiClient.captureContext()
        acceptingRegistrations = context != nil
    }

    /// Request notification permission and register for remote notifications.
    func requestPermission() async {
        let requestedContext = context
        do {
            let granted = try await UNUserNotificationCenter.current()
                .requestAuthorization(options: [.alert, .badge, .sound])
            permissionGranted = granted
            if granted, acceptingRegistrations,
               let requestedContext, apiClient?.isCurrent(requestedContext) == true {
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
            self.uploadToken(token)
        }
    }

    /// Called when APNs registration fails.
    nonisolated func didFailToRegisterForRemoteNotifications(error: Error) {
        AppLog.push.error("Registration failed: \(String(describing: error), privacy: .public)")
    }

    /// Capture identity before launching asynchronous HTTP work. Each callback
    /// belongs to the configured login, never whichever login exists on retry.
    private func uploadToken(_ token: String) {
        guard acceptingRegistrations, let apiClient, let context,
              apiClient.isCurrent(context) else { return }
        deviceToken = token
        let id = UUID()
        let upload = Task { @MainActor in
            defer { self.uploads[id] = nil }
            do {
                try await apiClient.postVoid(
                    "/api/mobile/push/register", body: ["device_token": token], context: context
                )
            } catch AuthError.staleIdentity {
                // The previous login ended; do not retry or mutate its successor.
            } catch {
                AppLog.push.error("Failed to register token with server: \(String(describing: error), privacy: .public)")
            }
        }
        uploads[id] = (context.generation, upload)
    }

    /// Stop accepting callbacks, drain uploads, then unregister using their
    /// captured identity. Cancellation alone cannot retract a server-side write.
    func unregister() async {
        await unregister(context: context)
    }

    func unregister(context capturedContext: MobileAuthenticationContext?) async {
        let capturedClient = apiClient
        if context?.generation == capturedContext?.generation { acceptingRegistrations = false }
        let pending = uploads.values.filter { $0.generation == capturedContext?.generation }.map { $0.task }
        for upload in pending { await upload.value }
        if let capturedClient, let capturedContext {
            do {
                try await capturedClient.postCleanup("/api/mobile/push/unregister", context: capturedContext)
            } catch {
                AppLog.push.error("Failed to unregister token with server: \(String(describing: error), privacy: .public)")
            }
        }
        // configure() may have installed a replacement login while we awaited.
        if context?.generation == capturedContext?.generation {
            context = nil
            deviceToken = nil
        }
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
