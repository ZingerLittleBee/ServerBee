import SwiftUI

struct ContentView: View {
    @Environment(AuthManager.self) private var authManager
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

            // Configure WS token refresher
            wsClient.tokenRefresher = { [weak authManager] in
                guard let authManager else { return nil }
                return try? await authManager.refreshAccessToken()
            }

            // Connect WebSocket
            wsClient.onMessage = { [weak serversViewModel] message in
                serversViewModel?.handleWSMessage(message)
            }
            if let serverUrl = authManager.serverUrl,
               let token = authManager.getAccessToken() {
                wsClient.connect(serverUrl: serverUrl, accessToken: token)
            }
        }
        .onDisappear {
            wsClient.close()
        }
    }
}

#Preview {
    ContentView()
        .environment(AuthManager())
        .environment(AlertsViewModel())
}
