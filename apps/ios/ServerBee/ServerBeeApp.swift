import SwiftUI

@main
struct ServerBeeApp: App {
    @State private var networkMonitor = NetworkMonitor()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(networkMonitor)
                .onAppear {
                    networkMonitor.start()
                }
        }
    }
}

/// Minimal root view used for build verification.
struct ContentView: View {
    @Environment(NetworkMonitor.self) private var networkMonitor

    var body: some View {
        VStack(spacing: 0) {
            OfflineBannerView(isConnected: networkMonitor.isConnected)
            Spacer()
            Text("ServerBee")
                .font(.title)
            Spacer()
        }
    }
}
