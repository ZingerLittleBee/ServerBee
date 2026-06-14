import SwiftUI

/// API key management. Each user manages their own keys. The plaintext key is
/// shown once on creation (one-time secret) and never again.
struct ApiKeysView: View {
    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = ApiKeysViewModel()
    @State private var showCreate = false
    @State private var pendingDelete: ApiKey?

    var body: some View {
        List {
            if let error = viewModel.actionError {
                Section {
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(Color.serverOffline)
                }
            }
            if viewModel.keys.isEmpty, !viewModel.isLoading {
                Section {
                    Text(String(localized: "No API keys yet."))
                        .foregroundStyle(.secondary)
                }
            }
            ForEach(viewModel.keys) { key in
                VStack(alignment: .leading, spacing: 4) {
                    Text(key.name).font(.body)
                    Text(verbatim: "serverbee_\(key.keyPrefix)…")
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                    Text(Formatters.formatRelativeTime(key.createdAt))
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
                .swipeActions {
                    Button(role: .destructive) { pendingDelete = key } label: {
                        Label(String(localized: "Revoke"), systemImage: "trash")
                    }
                }
            }
        }
        .overlay {
            if viewModel.isLoading, viewModel.keys.isEmpty { ProgressView() }
        }
        .navigationTitle(String(localized: "API Keys"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { showCreate = true } label: { Image(systemName: "plus") }
            }
        }
        .task { await viewModel.load(apiClient: apiClient) }
        .refreshable { await viewModel.load(apiClient: apiClient) }
        .sheet(isPresented: $showCreate) {
            CreateApiKeySheet(viewModel: viewModel)
        }
        .sheet(item: $viewModel.revealedKey) { key in
            RevealKeySheet(key: key)
        }
        .confirmationDialog(
            String(localized: "Revoke this key?"),
            isPresented: Binding(get: { pendingDelete != nil }, set: { if !$0 { pendingDelete = nil } }),
            titleVisibility: .visible
        ) {
            if let key = pendingDelete {
                Button(String(localized: "Revoke \(key.name)"), role: .destructive) {
                    Task { await viewModel.delete(id: key.id, apiClient: apiClient) }
                }
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(localized: "Any integration using this key will immediately stop working."))
        }
    }
}

// MARK: - Create

private struct CreateApiKeySheet: View {
    @Bindable var viewModel: ApiKeysViewModel
    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss

    @State private var name = ""
    @State private var error: String?
    @State private var working = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(String(localized: "Key name (e.g. CI pipeline)"), text: $name)
                        .autocorrectionDisabled()
                } footer: {
                    Text(String(localized: "Give the key a name so you can recognise it later."))
                }
                if let error {
                    Section {
                        Label(error, systemImage: "exclamationmark.triangle.fill")
                            .foregroundStyle(Color.serverOffline)
                    }
                }
            }
            .navigationTitle(String(localized: "New API Key"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(String(localized: "Cancel")) { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if working {
                        ProgressView()
                    } else {
                        Button(String(localized: "Create")) { Task { await create() } }
                            .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                }
            }
        }
    }

    private func create() async {
        working = true
        error = nil
        let failure = await viewModel.create(name: name.trimmingCharacters(in: .whitespaces), apiClient: apiClient)
        working = false
        if let failure {
            error = failure
        } else {
            dismiss()  // RevealKeySheet is presented by the parent via revealedKey
        }
    }
}

// MARK: - Reveal (one-time secret)

private struct RevealKeySheet: View {
    let key: ApiKey
    @Environment(\.dismiss) private var dismiss
    @State private var copied = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 16) {
                    Image(systemName: "key.horizontal.fill")
                        .font(.largeTitle)
                        .foregroundStyle(Color.brandAccent)
                        .padding(.top, 12)
                    Text(String(localized: "Copy your API key now"))
                        .font(.headline)
                    Text(String(localized: "This is the only time the full key is shown. Store it somewhere safe."))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)

                    if let plaintext = key.key {
                        Text(plaintext)
                            .font(.callout.monospaced())
                            .textSelection(.enabled)
                            .padding(12)
                            .frame(maxWidth: .infinity)
                            .background(Color(.secondarySystemBackground))
                            .clipShape(RoundedRectangle(cornerRadius: 10))

                        Button {
                            UIPasteboard.general.string = plaintext
                            copied = true
                        } label: {
                            Label(copied ? String(localized: "Copied") : String(localized: "Copy to Clipboard"),
                                  systemImage: copied ? "checkmark" : "doc.on.doc")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.borderedProminent)
                    }
                }
                .padding(16)
            }
            .background(Color(.systemGroupedBackground))
            .navigationTitle(key.name)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(String(localized: "Done")) { dismiss() }
                }
            }
        }
    }
}
