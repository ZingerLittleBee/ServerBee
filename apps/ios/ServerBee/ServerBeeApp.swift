import SwiftUI

@main
struct ServerBeeApp: App {
    @State private var authManager = AuthManager()
    @State private var apiClient = APIClient(baseURL: "")
    @AppStorage("theme") private var theme: String = AppTheme.system.rawValue

    private var selectedTheme: AppTheme {
        AppTheme(rawValue: theme) ?? .system
    }

    var body: some Scene {
        WindowGroup {
            Group {
                if authManager.isAuthenticated {
                    MainTabView()
                } else {
                    LoginPlaceholderView()
                }
            }
            .environment(authManager)
            .environment(\.apiClient, apiClient)
            .preferredColorScheme(selectedTheme.colorScheme)
        }
    }
}

// MARK: - Main Tab View

struct MainTabView: View {
    var body: some View {
        TabView {
            ServersPlaceholderView()
                .tabItem {
                    Label(String(localized: "common_nav_servers"), systemImage: "server.rack")
                }
            AlertsPlaceholderView()
                .tabItem {
                    Label(String(localized: "common_nav_alerts"), systemImage: "bell.badge")
                }
            SettingsView()
                .tabItem {
                    Label(String(localized: "common_nav_settings"), systemImage: "gearshape")
                }
        }
    }
}

// MARK: - Placeholder Views (for compilation; other units implement these)

struct LoginPlaceholderView: View {
    var body: some View {
        Text(String(localized: "login_title"))
            .font(.title)
    }
}

struct ServersPlaceholderView: View {
    var body: some View {
        NavigationStack {
            Text(String(localized: "servers_loading"))
                .navigationTitle(String(localized: "common_nav_servers"))
        }
    }
}

struct AlertsPlaceholderView: View {
    var body: some View {
        NavigationStack {
            Text(String(localized: "alerts_loading"))
                .navigationTitle(String(localized: "common_nav_alerts"))
        }
    }
}
