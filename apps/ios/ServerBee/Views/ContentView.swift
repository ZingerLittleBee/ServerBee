import SwiftUI

struct ContentView: View {
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
                Text("Alerts")
                    .navigationTitle(String(localized: "Alerts"))
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
