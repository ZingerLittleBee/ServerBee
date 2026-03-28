import SwiftUI

struct ContentView: View {
    @Environment(AlertsViewModel.self) private var alertsViewModel

    var body: some View {
        TabView {
            NavigationStack {
                Text("Servers")
                    .navigationTitle(String(localized: "Servers"))
            }
            .tabItem {
                Label(String(localized: "Servers"), systemImage: "server.rack")
            }

            NavigationStack {
                AlertsListView()
            }
            .tabItem {
                Label(String(localized: "Alerts"), systemImage: "bell.badge")
            }

            NavigationStack {
                Text("Settings")
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
        .environment(AlertsViewModel())
}
