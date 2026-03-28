import SwiftUI

struct ContentView: View {
    var body: some View {
        TabView {
            NavigationStack {
                Text("Servers")
                    .navigationTitle("Servers")
            }
            .tabItem {
                Label("Servers", systemImage: "server.rack")
            }

            NavigationStack {
                Text("Alerts")
                    .navigationTitle("Alerts")
            }
            .tabItem {
                Label("Alerts", systemImage: "bell.badge")
            }

            NavigationStack {
                Text("Settings")
                    .navigationTitle("Settings")
            }
            .tabItem {
                Label("Settings", systemImage: "gearshape")
            }
        }
    }
}

#Preview {
    ContentView()
}
