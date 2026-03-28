import SwiftUI

struct LoginView: View {
    @State private var viewModel = AuthViewModel()
    @Environment(AuthManager.self) private var authManager

    var body: some View {
        ScrollView {
            VStack(spacing: 24) {
                VStack(spacing: 8) {
                    Image(systemName: "server.rack")
                        .font(.system(size: 60))
                        .foregroundStyle(Color.accentColor)
                    Text("ServerBee")
                        .font(.largeTitle.bold())
                }
                .padding(.top, 60)
                .padding(.bottom, 20)

                VStack(spacing: 16) {
                    if viewModel.step == .credentials {
                        credentialsFields
                    } else {
                        totpFields
                    }

                    if !viewModel.errorMessage.isEmpty {
                        Text(viewModel.errorMessage)
                            .font(.subheadline)
                            .foregroundStyle(.red)
                            .multilineTextAlignment(.center)
                    }

                    Button {
                        Task {
                            await viewModel.login(authManager: authManager)
                        }
                    } label: {
                        Group {
                            if viewModel.isLoading {
                                ProgressView()
                                    .tint(.white)
                            } else {
                                Text("Login")
                                    .fontWeight(.semibold)
                            }
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 14)
                    }
                    .background(Color.accentColor)
                    .foregroundStyle(.white)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                    .disabled(viewModel.isLoading)

                    if viewModel.step == .totp {
                        Button(String(localized: "Back")) {
                            viewModel.goBackToCredentials()
                        }
                        .foregroundStyle(.secondary)
                    }
                }
                .padding(.horizontal, 24)
            }
        }
        .scrollDismissesKeyboard(.interactively)
    }

    // MARK: - Subviews

    private var credentialsFields: some View {
        Group {
            LabeledField(label: String(localized: "Server URL")) {
                TextField("https://your-server.com", text: $viewModel.serverUrlInput)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            }

            LabeledField(label: String(localized: "Username")) {
                TextField(String(localized: "Username"), text: $viewModel.username)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            }

            LabeledField(label: String(localized: "Password")) {
                SecureField(String(localized: "Password"), text: $viewModel.password)
            }
        }
    }

    private var totpFields: some View {
        VStack(spacing: 12) {
            Text("Two-Factor Authentication")
                .font(.headline)
            Text("Enter the 6-digit code from your authenticator app")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

            TextField("000000", text: $viewModel.totpCode)
                .textFieldStyle(.roundedBorder)
                .keyboardType(.numberPad)
                .multilineTextAlignment(.center)
                .font(.title2.monospaced())
        }
    }
}

// MARK: - Labeled Field

private struct LabeledField<Content: View>: View {
    let label: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label)
                .font(.subheadline.weight(.medium))
            content
                .textFieldStyle(.roundedBorder)
        }
    }
}
