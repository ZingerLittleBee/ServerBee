import SwiftUI

struct ContentView: View {
    @Environment(AuthManager.self) private var authManager
    @Environment(PushNotificationManager.self) private var pushManager
    @Environment(NetworkMonitor.self) private var networkMonitor
    @Environment(AlertsViewModel.self) private var alertsViewModel
    @Environment(\.scenePhase) private var scenePhase
    @State private var apiClient: APIClient?
    @State private var serversViewModel = ServersViewModel()
    @State private var wsClient = WebSocketClient()

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
            let client = APIClient(authManager: authManager)
            apiClient = client
            pushManager.configure(apiClient: client)

            await wsClient.setTokenRefresher { [weak authManager] in
                guard let authManager else { return nil }
                return try? await authManager.refreshAccessToken()
            }
            await wsClient.setOnMessage { [weak serversViewModel, weak alertsViewModel] message in
                Task { @MainActor in
                    guard let serversViewModel else { return }
                    let captureClient = apiClient
                    let router = WebSocketRouter(
                        servers: { msg in serversViewModel.handleWSMessage(msg) },
                        alerts: { msg in
                            guard case .alertEvent = msg, let alertsViewModel else { return }
                            if let captureClient {
                                Task { await alertsViewModel.handleWSAlertEvent(apiClient: captureClient) }
                            }
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
    ContentView()
        .environment(AuthManager())
        .environment(AlertsViewModel())
        .environment(PushNotificationManager())
        .environment(NetworkMonitor())
}
