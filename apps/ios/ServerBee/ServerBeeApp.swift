import SwiftUI

@main
struct ServerBeeApp: App {
    @State private var authManager = AuthManager()
    @State private var apiClient: APIClient?

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(authManager)
                .task {
                    if apiClient == nil {
                        apiClient = APIClient(authManager: authManager)
                    }
                    await authManager.initialize()
                }
        }
    }
}
