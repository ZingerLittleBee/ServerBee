import SwiftUI

/// Admin config of the global IP-quality system: the check interval and the
/// unlock-service catalog (enable/disable any service, delete custom ones).
/// Authoring a NEW custom service needs URL + JSON match-rule editing, which
/// stays on the web dashboard; mobile surfaces the high-value toggles. All
/// writes are admin-only (server-enforced).
struct IpQualityConfigView: View {
    let isAdmin: Bool

    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = IpQualityConfigViewModel()

    @State private var loaded = false
    @State private var savingSetting = false
    @State private var intervalHours = 12
    @State private var pendingDelete: UnlockService?

    var body: some View {
        List {
            if let error = viewModel.actionError ?? viewModel.loadError {
                Section {
                    Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline)
                }
            }
            if loaded {
                settingsSection
                servicesSection
            }
        }
        .overlay { if !loaded { ProgressView() } }
        .navigationTitle(String(localized: "IP Quality"))
        .navigationBarTitleDisplayMode(.inline)
        .task { await initialLoad() }
        .confirmationDialog(
            pendingDelete.map { String(format: String(localized: "Delete \"%@\"?"), $0.name) } ?? "",
            isPresented: Binding(get: { pendingDelete != nil }, set: { if !$0 { pendingDelete = nil } }),
            titleVisibility: .visible
        ) {
            Button(String(localized: "Delete"), role: .destructive) {
                if let service = pendingDelete {
                    Task { await viewModel.delete(service, apiClient: apiClient) }
                }
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        }
    }
}

private extension IpQualityConfigView {
    var settingsSection: some View {
        Section {
            Stepper(value: $intervalHours, in: 1...168) {
                LabeledContent(
                    String(localized: "Check interval"),
                    value: String(format: String(localized: "%dh"), intervalHours)
                )
            }
            if isAdmin {
                Button {
                    Task { await saveSetting() }
                } label: {
                    HStack {
                        Spacer()
                        if savingSetting { ProgressView() } else { Text(String(localized: "Save Settings")) }
                        Spacer()
                    }
                }
                .disabled(savingSetting)
            }
        } header: {
            Text(String(localized: "Global Settings"))
        } footer: {
            Text(String(localized: "How often agents re-check egress IP quality, in hours (1–168)."))
        }
    }

    @ViewBuilder
    var servicesSection: some View {
        ForEach(viewModel.groupedServices, id: \.category) { group in
            Section(group.category) {
                ForEach(group.services) { service in
                    serviceRow(service)
                        .swipeActions {
                            if isAdmin, !service.builtin {
                                Button(role: .destructive) { pendingDelete = service } label: {
                                    Label(String(localized: "Delete"), systemImage: "trash")
                                }
                            }
                        }
                }
            }
        }
        if !viewModel.services.isEmpty {
            Section {
                EmptyView()
            } footer: {
                Text(String(localized: "Create custom services (with match rules) in the web dashboard."))
            }
        }
    }

    func serviceRow(_ service: UnlockService) -> some View {
        HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(service.name).lineLimit(1)
                if !service.builtin {
                    Text(String(localized: "Custom"))
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(Color.brandAccent)
                }
            }
            Spacer(minLength: 8)
            Toggle("", isOn: Binding(
                get: { service.enabled },
                set: { value in Task { await viewModel.setEnabled(service, enabled: value, apiClient: apiClient) } }
            ))
            .labelsHidden()
            .accessibilityLabel(service.name)
            .disabled(!isAdmin)
        }
    }

    func initialLoad() async {
        guard !loaded else { return }
        await viewModel.load(apiClient: apiClient)
        if let setting = viewModel.setting { intervalHours = setting.checkIntervalHours }
        loaded = viewModel.loadError == nil
    }

    func saveSetting() async {
        savingSetting = true
        defer { savingSetting = false }
        _ = await viewModel.saveSetting(checkIntervalHours: intervalHours, apiClient: apiClient)
    }
}
