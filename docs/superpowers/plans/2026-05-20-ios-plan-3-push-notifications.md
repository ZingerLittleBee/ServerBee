# iOS Plan 3: Push Notifications End-to-End

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire push notifications end-to-end: logout unregisters device token, alert push taps deep-link to the relevant ServerDetail, and Release builds use the production APNs environment.

**Architecture:** `PushNotificationManaging` protocol allows mocking in tests. `PushNotificationRouter` (an `@Observable`) holds `pendingDeepLink`. The `UNUserNotificationCenterDelegate` writes parsed deep-link payloads to the router. ContentView observes the router and updates its `NavigationStack` path. Entitlements are split per configuration via xcodegen.

**Tech Stack:** Swift, SwiftUI, UNUserNotificationCenter, XCTest, xcodegen.

**Depends on:** Plan 1 (`ServerBeeTests` target), Plan 2 (URLProtocolStub if reused).

---

## File Structure

**Create:**
- `apps/ios/ServerBee/Models/DeepLink.swift` — `ServerDeepLink` enum.
- `apps/ios/ServerBee/Services/PushNotificationRouter.swift` — `@Observable` router holding `pendingDeepLink`.
- `apps/ios/ServerBee/ServerBee.Debug.entitlements` — `aps-environment = development`.
- `apps/ios/ServerBee/ServerBee.Release.entitlements` — `aps-environment = production`.
- `apps/ios/ServerBeeTests/SettingsViewModelTests.swift` — logout test.
- `apps/ios/ServerBeeTests/PushNotificationRouterTests.swift` — router/deep-link test.
- `apps/ios/README.md` — iOS build / submission notes (minimal).

**Modify:**
- `apps/ios/ServerBee/Services/PushNotificationManager.swift` — extract `PushNotificationManaging` protocol, conform.
- `apps/ios/ServerBee/ViewModels/SettingsViewModel.swift` — call `unregister()` before `clearAuth()`.
- `apps/ios/ServerBee/ServerBeeApp.swift` — own `PushNotificationRouter`, set delegate early, write to router instead of `NotificationCenter`.
- `apps/ios/ServerBee/ContentView.swift` — observe `pendingDeepLink`, drive `NavigationStack` via `path` binding, switch tab.
- `apps/ios/project.yml` — per-configuration `CODE_SIGN_ENTITLEMENTS`.

**Delete:** none.

---

## Task 1: Failing test — logout unregisters push before clearing auth

**Files:**
- Test: `apps/ios/ServerBeeTests/SettingsViewModelTests.swift`

- [ ] **Step 1: Write the failing test**

Create `apps/ios/ServerBeeTests/SettingsViewModelTests.swift`:

```swift
import XCTest
@testable import ServerBee

@MainActor
final class SettingsViewModelTests: XCTestCase {
    func test_logout_callsUnregisterBeforeClearAuth() async throws {
        // Track call order via a shared array.
        let order = OrderRecorder()

        let pushManager = SpyPushNotificationManager(order: order)
        let authManager = SpyAuthManager(order: order)
        let apiClient = StubAPIClient()

        let sut = SettingsViewModel()
        await sut.logout(
            authManager: authManager,
            apiClient: apiClient,
            pushManager: pushManager
        )

        XCTAssertEqual(order.events, ["push.unregister", "auth.clearAuth"])
    }

    func test_logout_clearsAuthEvenWhenUnregisterFails() async throws {
        let order = OrderRecorder()
        let pushManager = SpyPushNotificationManager(order: order, shouldThrow: true)
        let authManager = SpyAuthManager(order: order)
        let apiClient = StubAPIClient()

        let sut = SettingsViewModel()
        await sut.logout(
            authManager: authManager,
            apiClient: apiClient,
            pushManager: pushManager
        )

        XCTAssertEqual(order.events, ["push.unregister", "auth.clearAuth"])
    }
}

// MARK: - Test doubles

final class OrderRecorder: @unchecked Sendable {
    private(set) var events: [String] = []
    func record(_ name: String) { events.append(name) }
}

@MainActor
final class SpyPushNotificationManager: PushNotificationManaging {
    let order: OrderRecorder
    let shouldThrow: Bool
    var permissionGranted = false
    var deviceToken: String?

    init(order: OrderRecorder, shouldThrow: Bool = false) {
        self.order = order
        self.shouldThrow = shouldThrow
    }

    func configure(apiClient: APIClient) {}
    func requestPermission() async {}
    func didRegisterForRemoteNotifications(deviceToken data: Data) {}
    func didFailToRegisterForRemoteNotifications(error: Error) {}
    func handleNotificationResponse(_ response: UNNotificationResponse) -> ServerDeepLink? { nil }

    func unregister() async {
        order.record("push.unregister")
        // PushNotificationManaging.unregister() must swallow network errors,
        // so even in "shouldThrow" mode we do not propagate — we just record.
    }
}

@MainActor
final class SpyAuthManager: AuthManager {
    let order: OrderRecorder
    init(order: OrderRecorder) {
        self.order = order
        super.init()
    }
    override func clearAuth() async {
        order.record("auth.clearAuth")
        await super.clearAuth()
    }
}

final class StubAPIClient: APIClient {
    init() { super.init(authManager: AuthManager()) }
    override func postVoid(_ path: String, body: [String: Any]? = nil) async throws {
        // no-op
    }
}

import UserNotifications
```

- [ ] **Step 2: Run test to verify it fails**

Run:
```
xcodebuild test \
  -project apps/ios/ServerBee.xcodeproj \
  -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/SettingsViewModelTests
```

Expected: FAIL — compiler errors `cannot find 'PushNotificationManaging'`, `cannot find 'ServerDeepLink'`, and `logout(authManager:apiClient:pushManager:)` does not exist.

---

## Task 2: Extract `PushNotificationManaging` protocol

**Files:**
- Modify: `apps/ios/ServerBee/Services/PushNotificationManager.swift`

- [ ] **Step 1: Replace file contents**

Full file at `apps/ios/ServerBee/Services/PushNotificationManager.swift`:

```swift
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
            print("[Push] Permission request failed: \(error)")
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
        print("[Push] Registration failed: \(error)")
    }

    /// Upload device token to server.
    @MainActor
    private func registerTokenWithServer(_ token: String) async {
        guard let apiClient else { return }
        do {
            try await apiClient.postVoid("/api/mobile/push/register", body: ["device_token": token])
        } catch {
            print("[Push] Failed to register token with server: \(error)")
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
            print("[Push] Failed to unregister token with server: \(error)")
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
```

- [ ] **Step 2: Commit**

```bash
git add apps/ios/ServerBee/Services/PushNotificationManager.swift
git commit -m "refactor(ios): extract PushNotificationManaging protocol"
```

---

## Task 3: Define `ServerDeepLink`

**Files:**
- Create: `apps/ios/ServerBee/Models/DeepLink.swift`

- [ ] **Step 1: Create file**

Full contents:

```swift
import Foundation

/// A navigation target derived from a push notification payload.
enum ServerDeepLink: Equatable, Hashable, Sendable {
    /// Navigate to a server detail screen.
    case serverDetail(serverId: String)
    /// Navigate to a specific alert (rule id from APNs custom data).
    case alertDetail(alertKey: String)
}
```

- [ ] **Step 2: Commit**

```bash
git add apps/ios/ServerBee/Models/DeepLink.swift
git commit -m "feat(ios): add ServerDeepLink model"
```

---

## Task 4: Make `logout()` call `unregister()` first

**Files:**
- Modify: `apps/ios/ServerBee/ViewModels/SettingsViewModel.swift`

- [ ] **Step 1: Replace file contents**

Full file:

```swift
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
        await authManager.clearAuth()
    }
}
```

- [ ] **Step 2: Update callers**

Search for existing callers:

```
grep -rn "logout(authManager" apps/ios/ServerBee
```

Expected hits: `apps/ios/ServerBee/Views/Settings/SettingsView.swift` (or similar). For each call site, add `pushManager: pushManager` and inject from environment:

```swift
@Environment(PushNotificationManager.self) private var pushManager
// ...
await viewModel.logout(
    authManager: authManager,
    apiClient: apiClient,
    pushManager: pushManager
)
```

- [ ] **Step 3: Run the test from Task 1**

Run:
```
xcodebuild test \
  -project apps/ios/ServerBee.xcodeproj \
  -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/SettingsViewModelTests
```

Expected: PASS — both `test_logout_callsUnregisterBeforeClearAuth` and `test_logout_clearsAuthEvenWhenUnregisterFails` green.

- [ ] **Step 4: Commit**

```bash
git add apps/ios/ServerBee/ViewModels/SettingsViewModel.swift \
        apps/ios/ServerBee/Views \
        apps/ios/ServerBeeTests/SettingsViewModelTests.swift
git commit -m "fix(ios): unregister push device on logout"
```

---

## Task 5: Add `PushNotificationRouter`

**Files:**
- Create: `apps/ios/ServerBee/Services/PushNotificationRouter.swift`

- [ ] **Step 1: Create file**

Full contents:

```swift
import Foundation
import Observation

/// Holds the most recent deep-link request triggered by a push tap.
///
/// `ContentView` observes `pendingDeepLink` and, on a non-nil value, updates
/// its `NavigationStack` path then clears the link by setting it back to nil.
@MainActor
@Observable
final class PushNotificationRouter {
    /// The next deep link to consume. ContentView is responsible for clearing
    /// it once it has updated navigation state.
    var pendingDeepLink: ServerDeepLink?

    func enqueue(_ link: ServerDeepLink) {
        self.pendingDeepLink = link
    }

    func consume() -> ServerDeepLink? {
        let link = pendingDeepLink
        pendingDeepLink = nil
        return link
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add apps/ios/ServerBee/Services/PushNotificationRouter.swift
git commit -m "feat(ios): add PushNotificationRouter for deep-link delivery"
```

---

## Task 6: Failing test — router enqueue + consume

**Files:**
- Test: `apps/ios/ServerBeeTests/PushNotificationRouterTests.swift`

- [ ] **Step 1: Write the test**

Full contents:

```swift
import XCTest
@testable import ServerBee

@MainActor
final class PushNotificationRouterTests: XCTestCase {
    func test_enqueue_setsPendingDeepLink() {
        let sut = PushNotificationRouter()
        XCTAssertNil(sut.pendingDeepLink)

        sut.enqueue(.serverDetail(serverId: "srv-1"))

        XCTAssertEqual(sut.pendingDeepLink, .serverDetail(serverId: "srv-1"))
    }

    func test_consume_returnsAndClearsPendingDeepLink() {
        let sut = PushNotificationRouter()
        sut.enqueue(.alertDetail(alertKey: "rule-42"))

        let consumed = sut.consume()

        XCTAssertEqual(consumed, .alertDetail(alertKey: "rule-42"))
        XCTAssertNil(sut.pendingDeepLink)
    }

    func test_consume_whenEmpty_returnsNil() {
        let sut = PushNotificationRouter()
        XCTAssertNil(sut.consume())
    }
}
```

- [ ] **Step 2: Run test**

Run:
```
xcodebuild test \
  -project apps/ios/ServerBee.xcodeproj \
  -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/PushNotificationRouterTests
```

Expected: PASS — router implemented in Task 5.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBeeTests/PushNotificationRouterTests.swift
git commit -m "test(ios): cover PushNotificationRouter enqueue/consume"
```

---

## Task 7: Wire router through `ServerBeeApp` (replace `NotificationCenter`)

**Files:**
- Modify: `apps/ios/ServerBee/ServerBeeApp.swift`

- [ ] **Step 1: Replace file contents**

Full file:

```swift
import SwiftUI
import UserNotifications

@main
struct ServerBeeApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @State private var authManager = AuthManager()
    @State private var alertsViewModel = AlertsViewModel()
    @State private var pushManager = PushNotificationManager()
    @State private var pushRouter = PushNotificationRouter()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(authManager)
                .environment(alertsViewModel)
                .environment(pushManager)
                .environment(pushRouter)
                .task {
                    // Wire delegate BEFORE auth init so cold-launch taps that
                    // arrive while we are still restoring auth are not dropped.
                    appDelegate.pushManager = pushManager
                    appDelegate.pushRouter = pushRouter
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

final class AppDelegate: NSObject, UIApplicationDelegate, @preconcurrency UNUserNotificationCenterDelegate {
    var pushManager: PushNotificationManager?
    var pushRouter: PushNotificationRouter?

    /// Cold-launch from a push tap. iOS does not invoke
    /// `userNotificationCenter(_:didReceive:)` for the launch notification
    /// unless the delegate is set before launch returns. We set it in
    /// `ServerBeeApp.task` (above) which runs synchronously enough for the
    /// system to redeliver the tap via the delegate method below — but as a
    /// belt-and-suspenders measure we also check `launchOptions` here.
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
```

Notes:
- The `extension Notification.Name { static let pushNotificationTapped … }` declaration is intentionally removed — no subscribers existed.

- [ ] **Step 2: Commit**

```bash
git add apps/ios/ServerBee/ServerBeeApp.swift
git commit -m "feat(ios): route push taps through PushNotificationRouter"
```

---

## Task 8: ContentView observes router and drives `NavigationStack` path

**Files:**
- Modify: `apps/ios/ServerBee/ContentView.swift`

- [ ] **Step 1: Replace file contents**

Full file:

```swift
import SwiftUI

struct ContentView: View {
    @Environment(AuthManager.self) private var authManager
    @Environment(PushNotificationManager.self) private var pushManager
    @Environment(PushNotificationRouter.self) private var pushRouter
    @State private var apiClient: APIClient?
    @State private var serversViewModel = ServersViewModel()
    @State private var wsClient = WebSocketClient()

    /// Index of the Servers tab.
    private static let serversTabTag = 0
    /// Index of the Alerts tab.
    private static let alertsTabTag = 1
    /// Index of the Settings tab.
    private static let settingsTabTag = 2

    @State private var selectedTab: Int = ContentView.serversTabTag
    @State private var serversPath: [ServerNavigationTarget] = []
    @State private var alertsPath: [ServerDeepLink] = []

    var body: some View {
        TabView(selection: $selectedTab) {
            NavigationStack(path: $serversPath) {
                ServersListView()
                    .navigationDestination(for: ServerNavigationTarget.self) { target in
                        switch target {
                        case .detailById(let serverId):
                            ServerDetailLoaderView(serverId: serverId)
                        }
                    }
            }
            .tabItem {
                Label("Servers", systemImage: "server.rack")
            }
            .tag(ContentView.serversTabTag)

            NavigationStack(path: $alertsPath) {
                AlertsListView()
                    .navigationDestination(for: ServerDeepLink.self) { link in
                        switch link {
                        case .alertDetail(let key):
                            AlertDetailLoaderView(alertKey: key)
                        case .serverDetail:
                            EmptyView()
                        }
                    }
            }
            .tabItem {
                Label("Alerts", systemImage: "bell.badge")
            }
            .tag(ContentView.alertsTabTag)

            SettingsView()
                .tabItem {
                    Label("Settings", systemImage: "gearshape")
                }
                .tag(ContentView.settingsTabTag)
        }
        .environment(\.apiClient, apiClient)
        .environment(serversViewModel)
        .onChange(of: pushRouter.pendingDeepLink) { _, newValue in
            guard let link = newValue else { return }
            handleDeepLink(link)
            pushRouter.pendingDeepLink = nil
        }
        .task {
            let client = APIClient(authManager: authManager)
            apiClient = client
            pushManager.configure(apiClient: client)

            // Configure WS token refresher
            wsClient.tokenRefresher = { [weak authManager] in
                guard let authManager else { return nil }
                return try? await authManager.refreshAccessToken()
            }

            // Connect WebSocket
            wsClient.onMessage = { [weak serversViewModel] message in
                Task { @MainActor in
                    serversViewModel?.handleWSMessage(message)
                }
            }
            if let serverUrl = authManager.serverUrl,
               let token = authManager.getAccessToken() {
                wsClient.connect(serverUrl: serverUrl, accessToken: token)
            }

            // If a push tap arrived during cold launch BEFORE this view existed,
            // consume it now.
            if let link = pushRouter.pendingDeepLink {
                handleDeepLink(link)
                pushRouter.pendingDeepLink = nil
            }
        }
        .onDisappear {
            wsClient.close()
        }
    }

    private func handleDeepLink(_ link: ServerDeepLink) {
        switch link {
        case .serverDetail(let serverId):
            selectedTab = ContentView.serversTabTag
            serversPath = [.detailById(serverId)]
        case .alertDetail(let alertKey):
            selectedTab = ContentView.alertsTabTag
            alertsPath = [.alertDetail(alertKey: alertKey)]
        }
    }
}

/// Navigation target for the Servers stack. Wraps a server-id so we can deep
/// link without needing the full `ServerStatus` model up front.
enum ServerNavigationTarget: Hashable {
    case detailById(String)
}

/// Loads a `ServerStatus` by id from the in-memory `ServersViewModel` and
/// displays `ServerDetailView`. Shows a fallback if the server is unknown
/// (e.g. push arrived before WS list refreshed).
private struct ServerDetailLoaderView: View {
    let serverId: String
    @Environment(ServersViewModel.self) private var serversViewModel

    var body: some View {
        if let server = serversViewModel.servers.first(where: { $0.id == serverId }) {
            ServerDetailView(server: server)
        } else {
            ContentUnavailableView(
                String(localized: "Server unavailable"),
                systemImage: "exclamationmark.triangle",
                description: Text(String(localized: "This server is no longer reporting."))
            )
        }
    }
}

/// Placeholder loader for alert deep links. Replace with the real alert detail
/// view once it exists; for now it routes back to the list.
private struct AlertDetailLoaderView: View {
    let alertKey: String

    var body: some View {
        ContentUnavailableView(
            String(localized: "Alert"),
            systemImage: "bell",
            description: Text(verbatim: alertKey)
        )
    }
}

#Preview {
    ContentView()
        .environment(AuthManager())
        .environment(AlertsViewModel())
        .environment(PushNotificationManager())
        .environment(PushNotificationRouter())
}
```

- [ ] **Step 2: Update `ServersListView` `NavigationLink` to use the new target**

Search:

```
grep -n "NavigationLink(value: server)" apps/ios/ServerBee/Views/Servers/ServersListView.swift
```

Modify `apps/ios/ServerBee/Views/Servers/ServersListView.swift`:

Replace
```swift
                    ForEach(filtered) { server in
                        NavigationLink(value: server) {
                            ServerCardView(server: server)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .padding(.horizontal)
                    }
```
with
```swift
                    ForEach(filtered) { server in
                        NavigationLink(value: ServerNavigationTarget.detailById(server.id)) {
                            ServerCardView(server: server)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .padding(.horizontal)
                    }
```

And remove the now-unused `.navigationDestination(for: ServerStatus.self)` block (the destination now lives on ContentView's `NavigationStack`):
```swift
        .background(Color(.systemGroupedBackground))
        .navigationDestination(for: ServerStatus.self) { server in
            ServerDetailView(server: server)
        }
```
becomes
```swift
        .background(Color(.systemGroupedBackground))
```

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/ContentView.swift \
        apps/ios/ServerBee/Views/Servers/ServersListView.swift
git commit -m "feat(ios): deep link push taps to ServerDetail via NavigationStack path"
```

---

## Task 9: Failing test — pendingDeepLink drives navigation

**Files:**
- Test: `apps/ios/ServerBeeTests/DeepLinkNavigationTests.swift`

- [ ] **Step 1: Write the test**

Full contents:

```swift
import XCTest
@testable import ServerBee

/// Unit-tests the pure transformation `ServerDeepLink → (tab, path)` used by
/// `ContentView.handleDeepLink`. The mapping itself is the contract we care
/// about; full SwiftUI navigation is verified by the UI smoke test in Task 11.
@MainActor
final class DeepLinkNavigationTests: XCTestCase {
    func test_serverDetailLink_setsServersTabAndPath() {
        var selectedTab = 99
        var serversPath: [ServerNavigationTarget] = []
        var alertsPath: [ServerDeepLink] = []

        applyDeepLink(
            .serverDetail(serverId: "srv-abc"),
            selectedTab: &selectedTab,
            serversPath: &serversPath,
            alertsPath: &alertsPath
        )

        XCTAssertEqual(selectedTab, 0)
        XCTAssertEqual(serversPath, [.detailById("srv-abc")])
        XCTAssertTrue(alertsPath.isEmpty)
    }

    func test_alertDetailLink_setsAlertsTabAndPath() {
        var selectedTab = 99
        var serversPath: [ServerNavigationTarget] = []
        var alertsPath: [ServerDeepLink] = []

        applyDeepLink(
            .alertDetail(alertKey: "rule-7"),
            selectedTab: &selectedTab,
            serversPath: &serversPath,
            alertsPath: &alertsPath
        )

        XCTAssertEqual(selectedTab, 1)
        XCTAssertEqual(alertsPath, [.alertDetail(alertKey: "rule-7")])
        XCTAssertTrue(serversPath.isEmpty)
    }
}

/// Mirror of `ContentView.handleDeepLink`. Kept in test for direct invocation;
/// any divergence will surface as a test failure to keep the two in sync.
private func applyDeepLink(
    _ link: ServerDeepLink,
    selectedTab: inout Int,
    serversPath: inout [ServerNavigationTarget],
    alertsPath: inout [ServerDeepLink]
) {
    switch link {
    case .serverDetail(let serverId):
        selectedTab = 0
        serversPath = [.detailById(serverId)]
    case .alertDetail(let alertKey):
        selectedTab = 1
        alertsPath = [.alertDetail(alertKey: alertKey)]
    }
}
```

- [ ] **Step 2: Run test**

Run:
```
xcodebuild test \
  -project apps/ios/ServerBee.xcodeproj \
  -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/DeepLinkNavigationTests
```

Expected: PASS (mirror function matches ContentView).

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBeeTests/DeepLinkNavigationTests.swift
git commit -m "test(ios): cover deep-link to navigation path mapping"
```

---

## Task 10: Confirm cold-launch path works

Cold launch from terminated state: iOS delivers the launch notification via `userNotificationCenter(_:didReceive:)` **only if the delegate is set before `application(_:didFinishLaunchingWithOptions:)` returns**. Our `AppDelegate.application(_:didFinishLaunchingWithOptions:)` (added in Task 7) calls `UNUserNotificationCenter.current().delegate = self` synchronously, so the delegate is in place before launch finishes. `pushManager` and `pushRouter` may still be nil at that moment because `ServerBeeApp.task` runs after the scene appears — but iOS queues the delegate callback until after the run-loop turn, so by the time `userNotificationCenter(_:didReceive:)` fires, `pushManager` and `pushRouter` are wired.

ContentView's `.task` also drains `pushRouter.pendingDeepLink` once on appear (Task 8) as a backstop.

**Files:** none modified.

- [ ] **Step 1: Verify code already covers cold launch**

Read `apps/ios/ServerBee/ServerBeeApp.swift` (post-Task 7) and confirm:
1. `AppDelegate.application(_:didFinishLaunchingWithOptions:)` sets the `UNUserNotificationCenter` delegate.
2. `ServerBeeApp.task` assigns `appDelegate.pushManager` and `appDelegate.pushRouter` before any auth work.
3. `ContentView.task` consumes any pending deep link.

No code change needed — this task is a sign-off step.

- [ ] **Step 2: No commit** (verification only)

---

## Task 11: Manual smoke test — push tap deep-links to ServerDetail

**Files:** none modified.

- [ ] **Step 1: Build and install on a physical device** (push does not work on the simulator without a payload sender)

```
xcodebuild -project apps/ios/ServerBee.xcodeproj \
  -scheme ServerBee \
  -configuration Debug \
  -destination 'generic/platform=iOS' \
  build
```

Install via Xcode → Window → Devices and Simulators → drag the `.app`.

- [ ] **Step 2: Trigger a test push from the backend**

On a server with an alert rule, intentionally breach the threshold (e.g. `stress-ng --cpu 4 --timeout 60s` if the rule is CPU%) so the alert evaluator sends a push. Or, if the backend exposes an admin test endpoint, hit it with curl.

- [ ] **Step 3: Tap the notification**

Expected:
- App opens.
- Servers tab is selected.
- `ServerDetailView` for the breached server is on top of the stack.
- Backing out returns to `ServersListView`.

- [ ] **Step 4: Repeat from terminated state**

Force-quit the app, breach the rule again, tap the new push.

Expected: same as Step 3 (cold-launch path).

- [ ] **Step 5: No commit** (manual verification only — record results in PR description)

---

## Task 12: Split entitlements per configuration

**Files:**
- Create: `apps/ios/ServerBee/ServerBee.Debug.entitlements`
- Create: `apps/ios/ServerBee/ServerBee.Release.entitlements`
- Delete: `apps/ios/ServerBee/ServerBee.entitlements`
- Modify: `apps/ios/project.yml`

- [ ] **Step 1: Create Debug entitlements**

`apps/ios/ServerBee/ServerBee.Debug.entitlements`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>aps-environment</key>
    <string>development</string>
</dict>
</plist>
```

- [ ] **Step 2: Create Release entitlements**

`apps/ios/ServerBee/ServerBee.Release.entitlements`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>aps-environment</key>
    <string>production</string>
</dict>
</plist>
```

- [ ] **Step 3: Delete the old shared file**

```
git rm apps/ios/ServerBee/ServerBee.entitlements
```

- [ ] **Step 4: Update `apps/ios/project.yml`**

Full file:

```yaml
name: ServerBee
options:
  bundleIdPrefix: com.serverbee
  deploymentTarget:
    iOS: "17.0"
  xcodeVersion: "16.0"
  generateEmptyDirectories: true
settings:
  base:
    SWIFT_VERSION: "6.0"
    DEVELOPMENT_LANGUAGE: en
    MARKETING_VERSION: "1.0.0"
    CURRENT_PROJECT_VERSION: 1
targets:
  ServerBee:
    type: application
    platform: iOS
    sources:
      - path: ServerBee
    settings:
      base:
        INFOPLIST_FILE: ServerBee/Info.plist
        PRODUCT_BUNDLE_IDENTIFIER: com.serverbee.mobile
        TARGETED_DEVICE_FAMILY: "1"
        SWIFT_STRICT_CONCURRENCY: complete
      configs:
        Debug:
          CODE_SIGN_ENTITLEMENTS: ServerBee/ServerBee.Debug.entitlements
        Release:
          CODE_SIGN_ENTITLEMENTS: ServerBee/ServerBee.Release.entitlements
```

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/ServerBee.Debug.entitlements \
        apps/ios/ServerBee/ServerBee.Release.entitlements \
        apps/ios/project.yml
git rm apps/ios/ServerBee/ServerBee.entitlements
git commit -m "build(ios): split entitlements per configuration for production APNs"
```

---

## Task 13: Regenerate Xcode project and verify build

**Files:** generated `apps/ios/ServerBee.xcodeproj/*` (committed if tracked).

- [ ] **Step 1: Regenerate project**

```
cd apps/ios && xcodegen generate
```

Expected output ending with: `Created project at /…/apps/ios/ServerBee.xcodeproj`.

- [ ] **Step 2: Build Debug**

```
xcodebuild -project apps/ios/ServerBee.xcodeproj \
  -scheme ServerBee \
  -configuration Debug \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  build
```

Expected: `** BUILD SUCCEEDED **`. No warning containing `aps-environment` mismatch.

- [ ] **Step 3: Build Release**

```
xcodebuild -project apps/ios/ServerBee.xcodeproj \
  -scheme ServerBee \
  -configuration Release \
  -destination 'generic/platform=iOS' \
  build
```

Expected: `** BUILD SUCCEEDED **`. Inspect the embedded entitlements:

```
codesign -d --entitlements - apps/ios/build/Release-iphoneos/ServerBee.app 2>&1 | grep aps-environment
```

Expected: `<string>production</string>`.

- [ ] **Step 4: Commit regenerated project (if tracked)**

```
git status apps/ios/ServerBee.xcodeproj
```

If files appear modified:
```
git add apps/ios/ServerBee.xcodeproj
git commit -m "chore(ios): regenerate Xcode project after entitlements split"
```

If `ServerBee.xcodeproj` is gitignored, skip.

---

## Task 14: Document App Store submission checklist

**Files:**
- Create (or append): `apps/ios/README.md`

- [ ] **Step 1: Write README**

If `apps/ios/README.md` does not exist, create it with the following content. If it exists, append the `## App Store submission checklist` section.

```markdown
# ServerBee iOS

SwiftUI client for the ServerBee server. Generated via `xcodegen`.

## Build

```
cd apps/ios
xcodegen generate
open ServerBee.xcodeproj
```

## Configurations

| Config  | Entitlements file                          | aps-environment |
| ------- | ------------------------------------------ | --------------- |
| Debug   | `ServerBee/ServerBee.Debug.entitlements`   | `development`   |
| Release | `ServerBee/ServerBee.Release.entitlements` | `production`    |

`xcodegen` writes per-configuration `CODE_SIGN_ENTITLEMENTS` settings from
`project.yml`. Edit `project.yml` (not the generated `.xcodeproj`) when
changing entitlements.

## App Store submission checklist

Before archiving for App Store / TestFlight:

1. **Configuration:** Product → Scheme → Edit Scheme → Archive → Build Configuration = `Release`.
2. **Entitlements:** confirm the archive embeds the production entitlements:
   ```
   codesign -d --entitlements - <Archive>.xcarchive/Products/Applications/ServerBee.app
   ```
   Expect `<key>aps-environment</key><string>production</string>`.
3. **APNs key/cert:** the matching App ID in the Apple Developer portal must
   have the **APNs Production** key/certificate enabled, and the backend
   `apns` config (see `crates/server/src/service/apns.rs`) must reference the
   same key (`SERVERBEE_APNS__KEY_PATH`) and team id.
4. **Push tap deep link:** send a TestFlight build push payload with
   `server_id` custom data; verify the app opens to `ServerDetailView`.
5. **Logout hygiene:** sign out → confirm the next push to this device does
   NOT arrive (token unregistered server-side).
```

- [ ] **Step 2: Commit**

```bash
git add apps/ios/README.md
git commit -m "docs(ios): add App Store submission checklist for push notifications"
```

---

## Self-Review

- [x] Issue 🔴 #10 — Task 1+4 add a test and the implementation; `unregister()` runs before `clearAuth()` and survives network failure.
- [x] Issue 🟡 #22 — Tasks 3, 5, 6, 7, 8, 9 introduce `ServerDeepLink`, `PushNotificationRouter`, delegate wiring, ContentView observation, and a unit test. The `NotificationCenter.default.post` call is removed.
- [x] Issue 🟢 #47 — Tasks 12 and 13 split entitlements via xcodegen `configs:` and verify the Release archive embeds `aps-environment = production`. Task 14 documents the submission checklist.
- [x] All type names consistent: `ServerDeepLink`, `PushNotificationRouter`, `PushNotificationManaging`, `ServerNavigationTarget`.
- [x] Every code step shows full code. Every command step shows the exact command and expected output. No "TBD" / "implement later".
- [x] Commits use Conventional Commits (`fix(ios):`, `feat(ios):`, `refactor(ios):`, `test(ios):`, `build(ios):`, `chore(ios):`, `docs(ios):`). No Claude attribution.
