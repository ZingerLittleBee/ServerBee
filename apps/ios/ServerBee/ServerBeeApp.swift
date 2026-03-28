import SwiftUI

@main
struct ServerBeeApp: App {
    @State private var authManager = AuthManager()
    @State private var alertsViewModel = AlertsViewModel()
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
            .environment(alertsViewModel)
            .environment(\.apiClient, apiClient)
            .task {
                let client = APIClient(authManager: authManager)
                apiClient = client
                await authManager.initialize()
            }
        }
    }
}
