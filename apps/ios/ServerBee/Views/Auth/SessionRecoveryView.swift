import SwiftUI

struct SessionRecoveryView: View {
    @Environment(AuthManager.self) private var authManager
    @State private var viewModel = SessionRecoveryViewModel()
    @State private var confirmSelection = false

    var body: some View {
        NavigationStack {
            Form {
                if let identity = authManager.sessionRecovery {
                    Section {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Server")
                            Text(verbatim: identity.serverUrl)
                                .font(.footnote)
                                .foregroundStyle(.primary)
                                .fixedSize(horizontal: false, vertical: true)
                                .textSelection(.enabled)
                        }
                        LabeledContent("Account") { Text(verbatim: identity.username).foregroundStyle(.primary) }
                        if let selected = identity.selectedSessionId {
                            LabeledContent("Selected session") {
                                Text(verbatim: selected).font(.caption).foregroundStyle(.primary)
                            }
                        }
                    } header: {
                        Text("Saved session")
                    } footer: {
                        Text("Your saved session could not be signed out securely. Verify the original account to finish cleanup, then sign in again.")
                    }
                    if !viewModel.candidates.isEmpty { selectionSection }
                    Section {
                        TextField("Username", text: $viewModel.username)
                            .textContentType(.username)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                        SecureField("Password", text: $viewModel.password)
                            .textContentType(.password)
                        if viewModel.requiresTOTP {
                            TextField("Verification code", text: $viewModel.totpCode)
                                .textContentType(.oneTimeCode)
                                .keyboardType(.numberPad)
                        }
                        Button {
                            if viewModel.candidates.isEmpty {
                                Task { await viewModel.recover(authManager: authManager) }
                            } else { confirmSelection = true }
                        } label: {
                            HStack {
                                if viewModel.candidates.isEmpty { Text("Verify account and finish sign-out") } else { Text("Sign out selected session") }
                                if viewModel.isWorking { Spacer(); ProgressView() }
                            }
                        }
                        .disabled(viewModel.isWorking || viewModel.username.isEmpty || viewModel.password.isEmpty
                            || (viewModel.requiresTOTP && viewModel.totpCode.isEmpty)
                            || (!viewModel.candidates.isEmpty && viewModel.selectedSessionId == nil))
                    }
                    InsecureURLBanner(serverUrl: identity.serverUrl)
                    Section {
                        Button("Retry secure sign-out") {
                            Task { await viewModel.retryCleanup(authManager: authManager) }
                        }
                        .disabled(viewModel.isWorking)
                    } footer: {
                        Text("Only the original session is cleaned up. Your saved identity stays on this device until the server confirms cleanup.")
                    }
                }
                if let error = viewModel.errorMessage ?? authManager.recoveryError {
                    Section { Text(error).foregroundStyle(.red) }
                }
            }
            .disabled(viewModel.isWorking)
            .navigationTitle("Recover session")
            .task(id: authManager.sessionRecovery?.loginId) {
                viewModel.username = authManager.sessionRecovery?.username ?? ""
                viewModel.password = ""
                viewModel.totpCode = ""
            }
            .onDisappear { viewModel.password = ""; viewModel.totpCode = "" }
            .confirmationDialog("Sign out selected session?", isPresented: $confirmSelection, titleVisibility: .visible) {
                Button("Sign Out", role: .destructive) { Task { await viewModel.recover(authManager: authManager) } }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Only the selected session will be signed out. Other sessions stay signed in.")
            }
        }
    }
}

private extension SessionRecoveryView {
    var selectionSection: some View {
        Section {
            ForEach(viewModel.candidates) { candidate in
                Button {
                    viewModel.selectedSessionId = candidate.mobileSessionId
                } label: {
                    HStack(alignment: .top) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(candidate.deviceName).foregroundStyle(.primary)
                            Text(String(localized: "Created \(createdTime(candidate.createdAt))"))
                                .foregroundStyle(.secondary)
                            Text(String(localized: "Last used \(Formatters.formatRelativeTime(candidate.lastUsedAt))"))
                                .foregroundStyle(.secondary)
                            Text(verbatim: candidate.mobileSessionId).font(.caption2).foregroundStyle(.primary)
                        }
                        Spacer()
                        Image(systemName: viewModel.selectedSessionId == candidate.mobileSessionId ? "checkmark.circle.fill" : "circle")
                    }
                    .font(.caption)
                    .padding(.vertical, 4)
                }
                .accessibilityAddTraits(viewModel.selectedSessionId == candidate.mobileSessionId ? [.isSelected] : [])
                .disabled(authManager.sessionRecovery?.selectedSessionId != nil)
            }
        } header: {
            Text("Choose the original session")
        } footer: {
            Text("The saved credentials no longer identify a session. Choose the original session by its device name and dates. Nothing is selected automatically.")
        }
    }

    func createdTime(_ value: String) -> String {
        ISO8601DateFormatter.shared.date(from: value)?.formatted(date: .abbreviated, time: .shortened) ?? value
    }
}
