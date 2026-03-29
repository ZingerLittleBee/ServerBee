import SwiftUI
import UserNotifications

@main
struct ServerBeeApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @State private var authManager = AuthManager()
    @State private var alertsViewModel = AlertsViewModel()
    @State private var pushManager = PushNotificationManager()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(authManager)
                .environment(alertsViewModel)
                .environment(pushManager)
                .task {
                    appDelegate.pushManager = pushManager
                    UNUserNotificationCenter.current().delegate = appDelegate
                    await authManager.initialize()
                    if authManager.isAuthenticated {
                        await pushManager.requestPermission()
                    }
                }
        }
    }
}

/// Shows a loading spinner while auth state is restored, then either LoginView or ContentView.
private struct RootView: View {
    @Environment(AuthManager.self) private var authManager

    var body: some View {
        Group {
            if authManager.isLoading {
                ProgressView()
            } else if authManager.isAuthenticated {
                ContentView()
            } else {
                LoginView()
            }
        }
    }
}

// MARK: - AppDelegate

class AppDelegate: NSObject, UIApplicationDelegate, @preconcurrency UNUserNotificationCenterDelegate {
    var pushManager: PushNotificationManager?

    func application(
        _ application: UIApplication,
        didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
        pushManager?.didRegisterForRemoteNotifications(deviceToken: deviceToken)
    }

    func application(
        _ application: UIApplication,
        didFailToRegisterForRemoteNotificationsWithError error: Error
    ) {
        pushManager?.didFailToRegisterForRemoteNotifications(error: error)
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        if let serverId = pushManager?.handleNotificationResponse(response) {
            NotificationCenter.default.post(
                name: .pushNotificationTapped,
                object: nil,
                userInfo: ["server_id": serverId]
            )
        }
        completionHandler()
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        // Show notification even when app is in foreground
        completionHandler([.banner, .badge, .sound])
    }
}

extension Notification.Name {
    static let pushNotificationTapped = Notification.Name("pushNotificationTapped")
}
