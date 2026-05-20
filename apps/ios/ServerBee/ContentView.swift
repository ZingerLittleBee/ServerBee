import SwiftUI

struct ContentView: View {
    @Environment(AuthManager.self) private var authManager
    @Environment(PushNotificationManager.self) private var pushManager
    @Environment(\.scenePhase) private var scenePhase
    @State private var apiClient: APIClient?
    @State private var serversViewModel = ServersViewModel()
    @State private var wsClient = WebSocketClient()

    var body: some View {
        TabView {
            NavigationStack {
                ServersListView()
            }
            .tabItem {
                Label("Servers", systemImage: "server.rack")
            }

            NavigationStack {
                AlertsListView()
            }
            .tabItem {
                Label("Alerts", systemImage: "bell.badge")
            }

            SettingsView()
                .tabItem {
                    Label("Settings", systemImage: "gearshape")
                }
        }
        .environment(\.apiClient, apiClient)
        .environment(serversViewModel)
        .task {
            let client = APIClient(authManager: authManager)
            apiClient = client
            pushManager.configure(apiClient: client)

            await wsClient.setTokenRefresher { [weak authManager] in
                guard let authManager else { return nil }
                return try? await authManager.refreshAccessToken()
            }

            await wsClient.setOnMessage { [weak serversViewModel] message in
                Task { @MainActor in
                    serversViewModel?.handleWSMessage(message)
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
}
