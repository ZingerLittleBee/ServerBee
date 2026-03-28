import SwiftUI

@main
struct ServerBeeApp: App {
    @State private var authManager = AuthManager()
    @State private var apiClient: APIClient?

    var body: some Scene {
        WindowGroup {
            Group {
                if authManager.isLoading {
                    ProgressView("Loading...")
                } else {
                    ContentView()
                }
            }
            .environment(authManager)
            .task {
                apiClient = APIClient(authManager: authManager)
                await authManager.initialize()
            }
        }
    }
}

/// Placeholder root view — replaced by the navigation layer in later units.
struct ContentView: View {
    @Environment(AuthManager.self) private var authManager

    var body: some View {
        VStack(spacing: 16) {
            if authManager.isAuthenticated {
                Text("Authenticated as \(authManager.user?.username ?? "unknown")")
            } else {
                Text("Not authenticated")
            }
        }
        .padding()
    }
}
