import SwiftUI

struct SettingsView: View {
    @Environment(AuthManager.self) private var authManager
    @Environment(\.apiClient) private var apiClient
    @Environment(PushNotificationManager.self) private var pushManager
    @State private var viewModel = SettingsViewModel()

    /// Live WebSocket client owned by `ContentView`. Passed in so logout can
    /// close it before clearing auth and triggering the server logout.
    let wsClient: WebSocketClient

    private var isAdmin: Bool { authManager.user?.role.lowercased() == "admin" }

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
                accessSection
                if isAdmin { adminSection }
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
                PasswordChangeView()
            } label: {
                Label(String(localized: "Change Password"), systemImage: "key")
            }
            NavigationLink {
                TwoFactorView()
            } label: {
                Label(String(localized: "Two-Factor Auth"), systemImage: "lock.shield")
            }
            NavigationLink {
                FirewallBlocklistView()
            } label: {
                Label(String(localized: "Firewall Blocklist"), systemImage: "hand.raised")
            }
        }
    }

    private var accessSection: some View {
        Section(String(localized: "Access")) {
            NavigationLink {
                ApiKeysView()
            } label: {
                Label(String(localized: "API Keys"), systemImage: "key.horizontal")
            }
            NavigationLink {
                DevicesView()
            } label: {
                Label(String(localized: "Devices"), systemImage: "iphone")
            }
        }
    }

    private var adminSection: some View {
        Section(String(localized: "Admin")) {
            NavigationLink {
                ServerGroupsView()
            } label: {
                Label(String(localized: "Server Groups"), systemImage: "folder")
            }
            NavigationLink {
                PingTasksView(isAdmin: isAdmin)
            } label: {
                Label(String(localized: "Ping Tasks"), systemImage: "dot.radiowaves.left.and.right")
            }
            NavigationLink {
                UsersView()
            } label: {
                Label(String(localized: "Users"), systemImage: "person.2")
            }
            NavigationLink {
                AuditLogView()
            } label: {
                Label(String(localized: "Audit Log"), systemImage: "list.bullet.rectangle")
            }
            NavigationLink {
                RateLimitView()
            } label: {
                Label(String(localized: "Rate Limits"), systemImage: "speedometer")
            }
            NavigationLink {
                DatabasesView(isAdmin: isAdmin)
            } label: {
                Label(String(localized: "GeoIP & ASN"), systemImage: "globe")
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
