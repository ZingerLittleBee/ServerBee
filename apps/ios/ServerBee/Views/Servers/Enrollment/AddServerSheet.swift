import SwiftUI

/// Admin "Add Server" flow: name a new pending server, mint its first
/// enrollment code, and show the install command to run on the target host.
struct AddServerSheet: View {
    @Environment(\.apiClient) private var apiClient
    @Environment(AuthManager.self) private var authManager
    @Environment(\.dismiss) private var dismiss
    @State private var viewModel = AgentLifecycleViewModel()

    @State private var name = ""

    var body: some View {
        NavigationStack {
            Form {
                if let issued = viewModel.issued {
                    Section {
                        EnrollmentResultView(issued: issued)
                    } header: {
                        Text(String(localized: "Server created"))
                    }
                } else if let replay = viewModel.onboardingReplay {
                    Section {
                        Text(String(localized: "This onboarding request already created the server. The original plaintext code cannot be recovered."))
                        if let offer = replay.outstandingOffer {
                            Button(String(localized: "Replace outstanding offer")) {
                                Task {
                                    await viewModel.replaceOffer(
                                        serverId: replay.serverId,
                                        offerId: offer.id,
                                        serverUrl: authManager.serverUrl,
                                        apiClient: apiClient
                                    )
                                }
                            }
                        } else {
                            Text(String(localized: "No outstanding enrollment offer remains. Manage Agent Authority from the existing server."))
                                .foregroundStyle(.secondary)
                        }
                    } header: {
                        Text(String(localized: "Server already created"))
                    }
                } else {
                    Section {
                        TextField(String(localized: "Server name"), text: $name)
                            .autocorrectionDisabled()
                    } header: {
                        Text(String(localized: "New server"))
                    } footer: {
                        Text(String(localized: "Creates a pending server and a one-time enrollment code. Run the install command on the host to connect its agent."))
                    }
                    if let error = viewModel.errorMessage {
                        Section {
                            Label(error, systemImage: "exclamationmark.triangle.fill")
                                .foregroundStyle(Color.serverOffline)
                        }
                    }
                }
            }
            .navigationTitle(String(localized: "Add Server"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(viewModel.issued == nil ? String(localized: "Cancel") : String(localized: "Done")) {
                        viewModel.resetOnboarding()
                        dismiss()
                    }
                }
                if viewModel.issued == nil, viewModel.onboardingReplay == nil {
                    ToolbarItem(placement: .confirmationAction) {
                        if viewModel.isWorking {
                            ProgressView()
                        } else {
                            Button(String(localized: "Create")) { Task { await create() } }
                                .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
                        }
                    }
                }
            }
        }
    }

    private func create() async {
        await viewModel.createServer(
            name: name.trimmingCharacters(in: .whitespaces),
            serverUrl: authManager.serverUrl,
            apiClient: apiClient
        )
    }
}
