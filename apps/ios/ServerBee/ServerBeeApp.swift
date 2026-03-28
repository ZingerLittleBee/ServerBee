import SwiftUI

@main
struct ServerBeeApp: App {
    @State private var authManager = AuthManager()
    @State private var alertsViewModel = AlertsViewModel()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(authManager)
                .environment(alertsViewModel)
                .task {
                    await authManager.initialize()
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
