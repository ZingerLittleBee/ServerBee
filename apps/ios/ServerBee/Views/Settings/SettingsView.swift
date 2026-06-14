import SwiftUI

struct SettingsView: View {
    @Environment(AuthManager.self) private var authManager
    @Environment(\.apiClient) private var apiClient
    @Environment(PushNotificationManager.self) private var pushManager
    @State private var viewModel = SettingsViewModel()

    /// Live WebSocket client owned by `ContentView`. Passed in so logout can
    /// close it before clearing auth and triggering the server logout.
    let wsClient: WebSocketClient

    var body: some View {
        NavigationStack {
            List {
                if let url = authManager.serverUrl, !url.isEmpty {
                    Section {
                        InsecureURLBanner(serverUrl: url)
                            .listRowBackground(Color.clear)
                            .listRowSeparator(.hidden)
                    }
                }
                accountSection
                securitySection
                preferencesSection
                aboutSection
                logoutSection
            }
            .navigationTitle(String(localized: "Settings"))
            .confirmationDialog(
                String(localized: "Are you sure you want to log out?"),
                isPresented: $viewModel.showLogoutConfirmation,
                titleVisibility: .visible
            ) {
                Button(String(localized: "Log Out"), role: .destructive) {
                    Task {
                        await viewModel.logout(
                            authManager: authManager,
                            apiClient: apiClient,
                            pushManager: pushManager,
                            closeWebSocket: { await wsClient.close() }
                        )
                    }
                }
                Button(String(localized: "Cancel"), role: .cancel) {}
            }
        }
    }

    private var accountSection: some View {
        Section(String(localized: "Account")) {
            LabeledContent(String(localized: "Username")) {
                Text(authManager.user?.username ?? "-")
            }
            LabeledContent(String(localized: "Role")) {
                Text(authManager.user?.role.capitalized ?? "-")
            }
            LabeledContent(String(localized: "Server")) {
                Text(authManager.serverUrl ?? "-")
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            DeviceNameRow()
        }
    }

    private var securitySection: some View {
        Section(String(localized: "Security")) {
            NavigationLink {
                FirewallBlocklistView()
            } label: {
                Label(String(localized: "Firewall Blocklist"), systemImage: "hand.raised")
            }
        }
    }

    private var preferencesSection: some View {
        Section(String(localized: "Preferences")) {
            NavigationLink {
                AppearanceView()
            } label: {
                Label(String(localized: "Appearance"), systemImage: "paintbrush")
            }
        }
    }

    private var aboutSection: some View {
        Section(String(localized: "About")) {
            LabeledContent(String(localized: "Version")) {
                Text(appVersion)
            }
        }
    }

    private var logoutSection: some View {
        Section {
            Button(role: .destructive) {
                viewModel.showLogoutConfirmation = true
            } label: {
                HStack {
                    Spacer()
                    if viewModel.isLoggingOut {
                        ProgressView()
                    } else {
                        Text(String(localized: "Log Out"))
                    }
                    Spacer()
                }
            }
            .disabled(viewModel.isLoggingOut)
        }
    }

    private var appVersion: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0.0"
    }
}
