import SwiftUI

@main
struct ServerBeeApp: App {
    @State private var authManager = AuthManager()

    var body: some Scene {
        WindowGroup {
            Group {
                if authManager.isLoading {
                    ProgressView(String(localized: "Loading..."))
                } else if authManager.isAuthenticated {
                    ContentView()
                } else {
                    LoginView()
                }
            }
            .environment(authManager)
            .task {
                await authManager.initialize()
            }
        }
    }
}
