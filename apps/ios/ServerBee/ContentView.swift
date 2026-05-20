import SwiftUI

struct ContentView: View {
    @Environment(PushNotificationManager.self) private var pushManager
    @Environment(NetworkMonitor.self) private var networkMonitor
    @Environment(AlertsViewModel.self) private var alertsViewModel
    @Environment(\.scenePhase) private var scenePhase
    @State private var apiClient: APIClient
    @State private var serversViewModel = ServersViewModel()
    @State private var wsClient = WebSocketClient()

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
            TabView {
                NavigationStack { ServersListView() }
                    .tabItem { Label("Servers", systemImage: "server.rack") }
                NavigationStack { AlertsListView() }
                    .tabItem { Label("Alerts", systemImage: "bell.badge") }
                SettingsView()
                    .tabItem { Label("Settings", systemImage: "gearshape") }
            }
            .environment(\.apiClient, apiClient)
            .environment(serversViewModel)

            OfflineBannerView(isConnected: networkMonitor.isConnected)
                .animation(.easeInOut(duration: 0.2), value: networkMonitor.isConnected)
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
        }
        .onChange(of: scenePhase) { old, new in
            if old == .background && new == .active {
                Task { await wsClient.reconnectIfNeeded() }
            }
        }
    }
}

#Preview {
    ContentView(authManager: AuthManager())
        .environment(AlertsViewModel())
        .environment(PushNotificationManager())
        .environment(NetworkMonitor())
}
