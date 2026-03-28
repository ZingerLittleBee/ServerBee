import SwiftUI

struct ContentView: View {
    @Environment(AuthManager.self) private var authManager
    @State private var apiClient: APIClient?

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
        .task {
            apiClient = APIClient(authManager: authManager)
        }
    }
}

#Preview {
    ContentView()
        .environment(AuthManager())
        .environment(AlertsViewModel())
}
