import SwiftUI

/// Admin config of the global network-probe system: fleet-wide settings
/// (interval, packets, default targets) and the custom-target catalog. Preset
/// targets are shown read-only. All writes are admin-only (server-enforced).
struct NetworkProbeConfigView: View {
    let isAdmin: Bool

    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = NetworkProbeConfigViewModel()

    @State private var loaded = false
    @State private var savingSetting = false
    @State private var interval = 60
    @State private var packetCount = 10
    @State private var defaultTargetIds: Set<String> = []

    @State private var showCreate = false
    @State private var editing: NetworkProbeTarget?
    @State private var pendingDelete: NetworkProbeTarget?
    @State private var showDefaultsPicker = false

    private static let intervalOptions = [30, 60, 120, 300, 600]

    var body: some View {
        List {
            if let error = viewModel.actionError ?? viewModel.loadError {
                Section {
                    Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline)
                }
            }
            if loaded {
                settingsSection
                customTargetsSection
                presetTargetsSection
            }
        }
        .overlay { if !loaded { ProgressView() } }
        .navigationTitle(String(localized: "Network Probes"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { addToolbar }
        .task { await initialLoad() }
        .sheet(isPresented: $showCreate) {
            NetworkProbeTargetFormSheet(editing: nil, viewModel: viewModel) {}
        }
        .sheet(item: $editing) { target in
            NetworkProbeTargetFormSheet(editing: target, viewModel: viewModel) {}
        }
        .sheet(isPresented: $showDefaultsPicker) { defaultsPicker }
        .confirmationDialog(
            pendingDelete.map { String(format: String(localized: "Delete \"%@\"?"), $0.name) } ?? "",
            isPresented: Binding(get: { pendingDelete != nil }, set: { if !$0 { pendingDelete = nil } }),
            titleVisibility: .visible
        ) {
            Button(String(localized: "Delete"), role: .destructive) {
                if let target = pendingDelete {
                    Task { await viewModel.deleteTarget(id: target.id, apiClient: apiClient) }
                }
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(localized: "This removes the target and its history from every server it is assigned to."))
        }
    }
}

private extension NetworkProbeConfigView {
    var settingsSection: some View {
        Section {
            Picker(String(localized: "Interval"), selection: $interval) {
                ForEach(Self.intervalOptions, id: \.self) { Text(intervalLabel($0)).tag($0) }
            }
            Stepper(value: $packetCount, in: 5...20) {
                LabeledContent(String(localized: "Packets"), value: "\(packetCount)")
            }
            Button { showDefaultsPicker = true } label: {
                LabeledContent(String(localized: "Default targets")) {
                    Text("\(defaultTargetIds.count)").foregroundStyle(.secondary)
                }
            }
            .tint(.primary)
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
            Text(String(localized: "Default targets are assigned to newly enrolled servers. Interval is in seconds."))
        }
    }

    var customTargetsSection: some View {
        Section(String(localized: "Custom Targets")) {
            if viewModel.customTargets.isEmpty {
                Text(String(localized: "No custom targets.")).foregroundStyle(.secondary)
            }
            ForEach(viewModel.customTargets) { target in
                targetRow(target)
                    .swipeActions {
                        if isAdmin {
                            Button(role: .destructive) { pendingDelete = target } label: {
                                Label(String(localized: "Delete"), systemImage: "trash")
                            }
                            Button { editing = target } label: {
                                Label(String(localized: "Edit"), systemImage: "pencil")
                            }
                            .tint(.brandAccent)
                        }
                    }
            }
        }
    }

    @ViewBuilder
    var presetTargetsSection: some View {
        if !viewModel.presetTargets.isEmpty {
            Section {
                ForEach(viewModel.presetTargets) { targetRow($0) }
            } header: {
                Text(String(localized: "Preset Targets"))
            } footer: {
                Text(String(localized: "Preset targets ship with the server and can't be edited here."))
            }
        }
    }

    func targetRow(_ target: NetworkProbeTarget) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 6) {
                Text(target.name).font(.body)
                Text(target.probeTypeEnum.label)
                    .font(.caption2.weight(.semibold))
                    .padding(.horizontal, 5).padding(.vertical, 1)
                    .background(Color.secondary.opacity(0.15))
                    .clipShape(Capsule())
            }
            Text("\(NetworkProvider.label(for: target.provider)) · \(target.target)")
                .font(.caption).foregroundStyle(.secondary).lineLimit(1)
        }
    }

    var defaultsPicker: some View {
        NavigationStack {
            List(viewModel.targets) { target in
                Button { toggleDefault(target.id) } label: {
                    HStack {
                        VStack(alignment: .leading) {
                            Text(target.name).foregroundStyle(.primary)
                            Text(NetworkProvider.label(for: target.provider))
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        if defaultTargetIds.contains(target.id) {
                            Image(systemName: "checkmark").foregroundStyle(Color.brandAccent)
                        }
                    }
                }
            }
            .navigationTitle(String(localized: "Default Targets"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(String(localized: "Done")) { showDefaultsPicker = false }
                }
            }
        }
    }

    @ToolbarContentBuilder
    var addToolbar: some ToolbarContent {
        if isAdmin {
            ToolbarItem(placement: .topBarTrailing) {
                Button { showCreate = true } label: { Image(systemName: "plus") }
            }
        }
    }

    func intervalLabel(_ seconds: Int) -> String {
        seconds % 60 == 0 ? String(format: String(localized: "%dm"), seconds / 60)
                          : String(format: String(localized: "%ds"), seconds)
    }

    func toggleDefault(_ id: String) {
        if defaultTargetIds.contains(id) { defaultTargetIds.remove(id) } else { defaultTargetIds.insert(id) }
    }

    func initialLoad() async {
        guard !loaded else { return }
        await viewModel.load(apiClient: apiClient)
        if let setting = viewModel.setting {
            interval = setting.interval
            packetCount = setting.packetCount
            defaultTargetIds = Set(setting.defaultTargetIds)
        }
        loaded = viewModel.setting != nil || viewModel.loadError == nil
    }

    func saveSetting() async {
        savingSetting = true
        defer { savingSetting = false }
        _ = await viewModel.saveSetting(
            UpdateProbeSettingRequest(
                interval: interval, packetCount: packetCount, defaultTargetIds: Array(defaultTargetIds)
            ),
            apiClient: apiClient
        )
    }
}
