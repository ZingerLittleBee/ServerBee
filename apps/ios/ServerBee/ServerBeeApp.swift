import SwiftUI
import UserNotifications

@main
struct ServerBeeApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @State private var authManager = AuthManager()
    @State private var alertsViewModel = AlertsViewModel()
    @State private var pushManager = PushNotificationManager()
    @State private var pushRouter = PushNotificationRouter()
    @State private var networkMonitor = NetworkMonitor()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(authManager)
                .environment(alertsViewModel)
                .environment(pushManager)
                .environment(pushRouter)
                .environment(networkMonitor)
                .task {
                    // Wire delegate BEFORE auth init so cold-launch taps that
                    // arrive while we are still restoring auth are not dropped.
                    appDelegate.pushManager = pushManager
                    appDelegate.pushRouter = pushRouter
                    UNUserNotificationCenter.current().delegate = appDelegate
                    networkMonitor.start()

                    await authManager.initialize()
                    if authManager.isAuthenticated {
                        #if DEBUG
                        let isUITest = UITestSupport.seed != nil
                        #else
                        let isUITest = false
                        #endif
                        if !isUITest {
                            await pushManager.requestPermission()
                        }
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
                ContentView(authManager: authManager)
            } else {
                LoginView()
            }
        }
    }
}

// MARK: - AppDelegate

final class AppDelegate: NSObject, UIApplicationDelegate, @preconcurrency UNUserNotificationCenterDelegate {
    var pushManager: PushNotificationManager?
    var pushRouter: PushNotificationRouter?

    /// Cold-launch from a push tap. iOS does not invoke
    /// `userNotificationCenter(_:didReceive:)` for the launch notification
    /// unless the delegate is set before launch returns. We set it in
    /// `ServerBeeApp.task` (above) which runs synchronously enough for the
    /// system to redeliver the tap via the delegate method below — but as a
    /// belt-and-suspenders measure we also set it here in
    /// `didFinishLaunchingWithOptions`.
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        return true
    }

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

    @MainActor
    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        if let link = pushManager?.handleNotificationResponse(response) {
            pushRouter?.enqueue(link)
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
