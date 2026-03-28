import SwiftUI

struct ContentView: View {
    @Environment(AuthManager.self) private var authManager

    var body: some View {
        TabView {
            NavigationStack {
                ServersListView()
            }
            .tabItem {
                Label(String(localized: "Servers"), systemImage: "server.rack")
            }

            NavigationStack {
                Text(String(localized: "Alerts"))
                    .navigationTitle(String(localized: "Alerts"))
            }
            .tabItem {
                Label(String(localized: "Alerts"), systemImage: "bell.badge")
            }

            NavigationStack {
                Text(String(localized: "Settings"))
                    .navigationTitle(String(localized: "Settings"))
            }
            .tabItem {
                Label(String(localized: "Settings"), systemImage: "gearshape")
            }
        }
    }
}

#Preview {
    ContentView()
        .environment(AuthManager())
}
