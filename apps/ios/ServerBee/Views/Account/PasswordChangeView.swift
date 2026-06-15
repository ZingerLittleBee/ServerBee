import SwiftUI

/// Change-password form. The server verifies the current password, enforces a
/// minimum length, and revokes the user's *other* sessions on success.
struct PasswordChangeView: View {
    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss
    @State private var viewModel = AccountSecurityViewModel()

    @State private var current = ""
    @State private var newPassword = ""
    @State private var confirm = ""
    @State private var errorMessage: String?
    @State private var didSucceed = false

    private var canSubmit: Bool {
        !current.isEmpty && newPassword.count >= 8 && newPassword == confirm && !viewModel.isWorking
    }

    var body: some View {
        Form {
            Section {
                SecureField(String(localized: "Current password"), text: $current)
                    .textContentType(.password)
            } header: {
                Text(String(localized: "Current"))
            }

            Section {
                SecureField(String(localized: "New password"), text: $newPassword)
                    .textContentType(.newPassword)
                SecureField(String(localized: "Confirm new password"), text: $confirm)
                    .textContentType(.newPassword)
            } header: {
                Text(String(localized: "New password"))
            } footer: {
                if !newPassword.isEmpty, newPassword.count < 8 {
                    Text(String(localized: "Must be at least 8 characters."))
                        .foregroundStyle(Color.serverOffline)
                } else if !confirm.isEmpty, newPassword != confirm {
                    Text(String(localized: "Passwords don't match."))
                        .foregroundStyle(Color.serverOffline)
                } else {
                    Text(String(localized: "Signing in elsewhere will be required again after changing your password."))
                }
            }

            if let errorMessage {
                Section {
                    Label(errorMessage, systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(Color.serverOffline)
                }
            }
        }
        .navigationTitle(String(localized: "Change Password"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                if viewModel.isWorking {
                    ProgressView()
                } else {
                    Button(String(localized: "Save")) { Task { await submit() } }
                        .disabled(!canSubmit)
                }
            }
        }
        .alert(String(localized: "Password changed"), isPresented: $didSucceed) {
            Button(String(localized: "Done")) { dismiss() }
        } message: {
            Text(String(localized: "Your password has been updated."))
        }
    }

    private func submit() async {
        errorMessage = nil
        if let failure = await viewModel.changePassword(old: current, new: newPassword, apiClient: apiClient) {
            errorMessage = failure
        } else {
            didSucceed = true
        }
    }
}
