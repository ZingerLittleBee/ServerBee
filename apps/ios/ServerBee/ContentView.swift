import SwiftUI

struct ContentView: View {
    @Environment(PushNotificationManager.self) private var pushManager
    @Environment(PushNotificationRouter.self) private var pushRouter
    @Environment(NetworkMonitor.self) private var networkMonitor
    @Environment(AlertsViewModel.self) private var alertsViewModel
    @Environment(\.scenePhase) private var scenePhase
    @State private var apiClient: APIClient
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

    private let authManager: AuthManager

    init(authManager: AuthManager) {
        self.authManager = authManager
        // Construct APIClient synchronously so child views' .task closures
        // never observe a nil client on first cold start.
        _apiClient = State(initialValue: APIClient(authManager: authManager))
    }

    /// Test-only accessor — assert the client was built during init.
    var apiClientForTest: APIClient { apiClient }

    var body: some View {
        ZStack(alignment: .top) {
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

            OfflineBannerView(isConnected: networkMonitor.isConnected)
                .animation(.easeInOut(duration: 0.2), value: networkMonitor.isConnected)
        }
        .onChange(of: pushRouter.pendingDeepLink) { _, newValue in
            guard let link = newValue else { return }
            handleDeepLink(link)
            pushRouter.pendingDeepLink = nil
        }
        .task {
            pushManager.configure(apiClient: apiClient)

            await wsClient.setTokenRefresher { [weak authManager] in
                guard let authManager else { return nil }
                return try? await authManager.refreshAccessToken()
            }
            await wsClient.setOnMessage {
                [weak serversViewModel, weak alertsViewModel, apiClient] message in
                Task { @MainActor in
                    guard let serversViewModel else { return }
                    let router = WebSocketRouter(
                        servers: { msg in serversViewModel.handleWSMessage(msg) },
                        alerts: { msg in
                            guard case .alertEvent = msg, let alertsViewModel else { return }
                            Task { await alertsViewModel.handleWSAlertEvent(apiClient: apiClient) }
                        }
                    )
                    router.dispatch(message)
                }
            }
            if let serverUrl = authManager.serverUrl,
               let token = authManager.getAccessToken() {
                await wsClient.connect(serverUrl: serverUrl, accessToken: token)
            }

            // If a push tap arrived during cold launch BEFORE this view existed,
            // consume it now.
            if let link = pushRouter.pendingDeepLink {
                handleDeepLink(link)
                pushRouter.pendingDeepLink = nil
            }
        }
        .onChange(of: scenePhase) { old, new in
            if old == .background && new == .active {
                Task { await wsClient.reconnectIfNeeded() }
            }
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
    ContentView(authManager: AuthManager())
        .environment(AlertsViewModel())
        .environment(PushNotificationManager())
        .environment(PushNotificationRouter())
        .environment(NetworkMonitor())
}
