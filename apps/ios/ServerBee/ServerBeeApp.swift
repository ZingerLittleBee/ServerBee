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
    @State private var securityFeed = SecurityFeedStore()
    @State private var upgradeJobs = UpgradeJobsStore()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(authManager)
                .environment(alertsViewModel)
                .environment(pushManager)
                .environment(pushRouter)
                .environment(networkMonitor)
                .environment(securityFeed)
                .environment(upgradeJobs)
                .task {
                    // Wire delegate BEFORE auth init so cold-launch taps that
                    // arrive while we are still restoring auth are not dropped.
                    appDelegate.pushManager = pushManager
                    appDelegate.pushRouter = pushRouter
                    UNUserNotificationCenter.current().delegate = appDelegate
                    networkMonitor.start()

                    await authManager.initialize()
                }
                .onChange(of: scenePhase) { _, phase in
                    if phase == .active { Task { await recoverSignOuts() } }
                }
                .onChange(of: networkMonitor.isConnected) { _, connected in
                    if connected { Task { await recoverSignOuts() } }
                }
        }
    }
    @MainActor
    private func recoverSignOuts() async {
        await authManager.retryPendingRevocations()
        if authManager.isAuthenticated { await pushManager.reconcile() }
    }
}

/// Shows a loading spinner while auth state is restored, then either LoginView or ContentView.
private struct RootView: View {
    @Environment(AuthManager.self) private var authManager
    @AppStorage("theme") private var theme: String = AppTheme.system.rawValue
    @AppStorage(PrivacyMode.storageKey) private var privacyMode = false

    var body: some View {
        Group {
            if authManager.isLoading {
                ProgressView()
            } else if authManager.sessionRecovery != nil {
                SessionRecoveryView()
            } else if authManager.isAuthenticated {
                ContentView(authManager: authManager)
            } else {
                LoginView()
            }
        }
        .safeAreaInset(edge: .bottom) {
            if authManager.sessionRecovery == nil, let error = authManager.recoveryError {
                Text(error).font(.footnote).foregroundStyle(.red).padding()
            }
        }
        // Applied at the root so the Appearance choice covers every screen.
        .preferredColorScheme((AppTheme(rawValue: theme) ?? .system).colorScheme)
        .environment(\.privacyMode, privacyMode)
    }
}

// MARK: - AppDelegate

@MainActor
final class AppDelegate: NSObject, UIApplicationDelegate, @preconcurrency UNUserNotificationCenterDelegate {
    var pushManager: PushNotificationManager? {
        didSet {
            if let pendingToken { pushManager?.didRegisterForRemoteNotifications(deviceToken: pendingToken) }
            pendingToken = nil
        }
    }
    private var pendingToken: Data?
    private var pendingEnvelope: PushEnvelope?
    var pushRouter: PushNotificationRouter? {
        didSet {
            if let pendingEnvelope { pushRouter?.enqueue(envelope: pendingEnvelope) }
            pendingEnvelope = nil
        }
    }

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
        if let pushManager { pushManager.didRegisterForRemoteNotifications(deviceToken: deviceToken) } else { pendingToken = deviceToken }
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
        bufferNotification(userInfo: response.notification.request.content.userInfo)
        completionHandler()
    }

    @MainActor
    func bufferNotification(userInfo: [AnyHashable: Any]) {
        guard let object = userInfo["serverbee_envelope"], JSONSerialization.isValidJSONObject(object),
              let data = try? JSONSerialization.data(withJSONObject: object), data.count <= 4096,
              let envelope = try? JSONDecoder().decode(PushEnvelope.self, from: data) else { return }
        if let pushRouter { pushRouter.enqueue(envelope: envelope) } else { pendingEnvelope = envelope }
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
