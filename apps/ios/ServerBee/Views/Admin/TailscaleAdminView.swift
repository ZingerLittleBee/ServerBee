import SwiftUI

/// Settings → Administration → Tailscale screen.
/// Allows admins to configure Tailscale / Headscale control-plane credentials
/// and Tailscale Serve loopback auth (`/api/tailscale/config`), test connectivity
/// (`/api/tailscale/test`), and inspect the live Tailnet device list
/// (`/api/tailscale/overview`).
struct TailscaleAdminView: View {
    let isAdmin: Bool

    @Environment(\.apiClient) private var apiClient
    @Environment(\.privacyMode) private var privacyMode
    @State private var viewModel = TailscaleAdminViewModel()

    @State private var loaded = false
    @State private var enabled = false
    @State private var provider: TailscaleProvider = .tailscale
    @State private var apiBaseUrl = "https://api.tailscale.com"
    @State private var tailnet = "-"
    @State private var apiKey = ""
    @State private var clearSavedApiKey = false
    @State private var preferredServerUrl = ""
    @State private var authEnabled = false
    @State private var autoProvisionRole: TailscaleAutoProvisionRole = .disabled

    var body: some View {
        Form {
            if let error = viewModel.actionError ?? viewModel.loadError {
                Section {
                    Label(error.maskingIPs(privacyMode), systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(Color.serverOffline)
                }
            }
            if loaded {
                if isAdmin {
                    controlPlaneSection
                    serveAuthSection
                    testConnectionSection
                }
                TailnetOverviewSection(
                    overview: viewModel.overview,
                    devices: viewModel.sortedDevices
                )
            }
        }
        .overlay {
            if !loaded && viewModel.isLoading {
                ProgressView()
            }
        }
        .navigationTitle(String(localized: "Tailscale"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { toolbarContent }
        .refreshable {
            await viewModel.load(isAdmin: isAdmin, apiClient: apiClient)
            syncFormFromConfig()
        }
        .task {
            await initialLoad()
        }
    }
}

extension TailscaleAdminView {
    private var controlPlaneSection: some View {
        Section {
            Toggle(String(localized: "Enable Tailscale integration"), isOn: $enabled)

            Picker(String(localized: "Provider"), selection: $provider) {
                ForEach(TailscaleProvider.allCases) { item in
                    Text(verbatim: item.label).tag(item)
                }
            }
            .onChange(of: provider) { oldValue, newValue in
                if apiBaseUrl.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                    || apiBaseUrl == oldValue.defaultApiBaseUrl
                {
                    apiBaseUrl = newValue.defaultApiBaseUrl
                }
            }

            LabeledContent(String(localized: "API Base URL")) {
                TextField(
                    text: $apiBaseUrl,
                    prompt: Text(
                        verbatim: provider == .headscale
                            ? "https://headscale.example.com"
                            : "https://api.tailscale.com"
                    )
                ) {
                    EmptyView()
                }
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .multilineTextAlignment(.trailing)
            }

            LabeledContent(String(localized: "Tailnet")) {
                TextField(text: $tailnet, prompt: Text(verbatim: "-")) {
                    EmptyView()
                }
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .multilineTextAlignment(.trailing)
            }

            SecureField(
                hasStoredKey && !clearSavedApiKey
                    ? String(localized: "API key saved — leave blank to keep")
                    : String(localized: "API key"),
                text: $apiKey
            )
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .onChange(of: apiKey) { _, newValue in
                if !newValue.isEmpty { clearSavedApiKey = false }
            }

            if hasStoredKey {
                Toggle(String(localized: "Clear saved API key"), isOn: $clearSavedApiKey)
                    .onChange(of: clearSavedApiKey) { _, shouldClear in
                        if shouldClear { apiKey = "" }
                    }
            }

            LabeledContent(String(localized: "Preferred Server URL")) {
                TextField(
                    text: $preferredServerUrl,
                    prompt: Text(verbatim: "https://serverbee.tailnet.ts.net")
                ) {
                    EmptyView()
                }
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .multilineTextAlignment(.trailing)
            }
        } header: {
            Text(String(localized: "Control Plane"))
        } footer: {
            Text(
                String(
                    localized: """
                        Connects to Tailscale or self-hosted Headscale to discover Tailnet devices and match \
                        enrolled ServerBee hosts. Preferred Server URL is offered when generating agent install commands.
                        """))
        }
    }

    private var serveAuthSection: some View {
        Section {
            Toggle(String(localized: "Tailscale Serve loopback auth"), isOn: $authEnabled)
            Picker(String(localized: "Auto-provision role"), selection: $autoProvisionRole) {
                ForEach(TailscaleAutoProvisionRole.allCases) { option in
                    Text(option.label).tag(option)
                }
            }
            .disabled(!authEnabled)
        } header: {
            Text(String(localized: "Tailscale Serve Auth"))
        } footer: {
            Text(
                String(
                    localized: """
                        Accepts Tailscale-User-Login identity headers only from local loopback connections \
                        without proxy forwarding headers. Admin accounts are never auto-provisioned.
                        """))
        }
    }

    private var testConnectionSection: some View {
        Section {
            Button {
                Task { await runConnectionTest() }
            } label: {
                HStack(spacing: 8) {
                    if viewModel.isTesting {
                        ProgressView().controlSize(.small)
                    } else {
                        Image(systemName: "bolt.horizontal.circle")
                    }
                    Text(String(localized: "Test Connection"))
                }
            }
            .disabled(viewModel.isTesting || viewModel.isSaving)

            if let result = viewModel.testResult {
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: result.success ? "checkmark.circle.fill" : "xmark.octagon.fill")
                        .foregroundStyle(result.success ? Color.serverOnline : Color.serverOffline)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: result.message.maskingIPs(privacyMode))
                            .font(.subheadline)
                        if result.success {
                            Text(String(localized: "\(result.deviceCount) devices reachable"))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .monospacedDigit()
                        }
                    }
                }
            }

            if viewModel.saveSucceeded {
                Label(String(localized: "Settings saved"), systemImage: "checkmark.circle.fill")
                    .font(.footnote.weight(.medium))
                    .foregroundStyle(Color.serverOnline)
            }
        }
    }

    @ToolbarContentBuilder
    private var toolbarContent: some ToolbarContent {
        ToolbarItem(placement: .confirmationAction) {
            if viewModel.isSaving {
                ProgressView()
            } else if isAdmin {
                Button(String(localized: "Save")) {
                    Task { await saveConfig() }
                }
                .disabled(!loaded)
            }
        }
    }

    private var hasStoredKey: Bool {
        viewModel.config?.hasApiKey ?? false
    }

    private func initialLoad() async {
        guard !loaded else { return }
        await viewModel.load(isAdmin: isAdmin, apiClient: apiClient)
        syncFormFromConfig()
        loaded = true
    }

    private func syncFormFromConfig() {
        guard let config = viewModel.config else { return }
        enabled = config.enabled
        provider = TailscaleProvider(rawValue: config.provider.lowercased()) ?? .tailscale
        apiBaseUrl = config.apiBaseUrl
        tailnet = config.tailnet
        apiKey = ""
        clearSavedApiKey = false
        preferredServerUrl = config.preferredServerUrl
        authEnabled = config.authEnabled
        autoProvisionRole = TailscaleAutoProvisionRole(rawValue: config.autoProvisionRole.lowercased()) ?? .disabled
    }

    private var outgoingApiKey: String? {
        if clearSavedApiKey { return "" }
        let trimmed = apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    private func saveConfig() async {
        let request = UpdateTailscaleConfigRequest(
            enabled: enabled,
            provider: provider.rawValue,
            apiBaseUrl: apiBaseUrl.trimmingCharacters(in: .whitespacesAndNewlines),
            tailnet: tailnet.trimmingCharacters(in: .whitespacesAndNewlines),
            apiKey: outgoingApiKey,
            preferredServerUrl: preferredServerUrl.trimmingCharacters(in: .whitespacesAndNewlines),
            authEnabled: authEnabled,
            autoProvisionRole: autoProvisionRole.rawValue
        )
        let ok = await viewModel.save(request, apiClient: apiClient)
        if ok {
            syncFormFromConfig()
        }
    }

    private func runConnectionTest() async {
        let trimmedKey = apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
        let request = TestTailscaleConfigRequest(
            provider: provider.rawValue,
            apiBaseUrl: apiBaseUrl.trimmingCharacters(in: .whitespacesAndNewlines),
            tailnet: tailnet.trimmingCharacters(in: .whitespacesAndNewlines),
            apiKey: trimmedKey.isEmpty ? nil : trimmedKey
        )
        _ = await viewModel.testConnection(request, apiClient: apiClient)
    }
}
