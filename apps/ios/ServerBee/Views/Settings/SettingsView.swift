import SwiftUI

struct SettingsView: View {
    @Environment(AuthManager.self) private var authManager
    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = SettingsViewModel()

    var body: some View {
        NavigationStack {
            List {
                accountSection
                preferencesSection
                aboutSection
                logoutSection
            }
            .navigationTitle(String(localized: "settings_title"))
            .confirmationDialog(
                String(localized: "settings_logout_confirm"),
                isPresented: $viewModel.showLogoutConfirmation,
                titleVisibility: .visible
            ) {
                Button(String(localized: "settings_logout"), role: .destructive) {
                    Task {
                        await viewModel.logout(authManager: authManager, apiClient: apiClient)
                    }
                }
                Button(String(localized: "settings_cancel"), role: .cancel) {}
            }
        }
    }

    private var accountSection: some View {
        Section(String(localized: "settings_account")) {
            LabeledContent(String(localized: "settings_username")) {
                Text(authManager.user?.username ?? "-")
            }
            LabeledContent(String(localized: "settings_role")) {
                Text(authManager.user?.role.capitalized ?? "-")
            }
            LabeledContent(String(localized: "settings_server")) {
                Text(authManager.serverUrl ?? "-")
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
    }

    private var preferencesSection: some View {
        Section(String(localized: "settings_preferences")) {
            NavigationLink {
                AppearanceView()
            } label: {
                Label(String(localized: "settings_appearance"), systemImage: "paintbrush")
            }
        }
    }

    private var aboutSection: some View {
        Section(String(localized: "settings_about")) {
            LabeledContent(String(localized: "settings_version")) {
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
                        Text(String(localized: "settings_logout"))
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
