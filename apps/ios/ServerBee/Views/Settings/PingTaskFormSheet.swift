import SwiftUI

/// Create or edit a ping task (admin). Mirrors the web ping-task form: name,
/// probe type, target, interval, the servers that run it (none = all), and an
/// enable toggle. Modeled on `MaintenanceFormSheet`.
struct PingTaskFormSheet: View {
    /// The task being edited, or nil to create a new one.
    let editing: PingTask?
    @Bindable var viewModel: PingTasksViewModel
    let onSaved: () -> Void

    @Environment(ServersViewModel.self) private var serversViewModel
    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss

    @State private var name = ""
    @State private var probeType: PingProbeType = .icmp
    @State private var target = ""
    @State private var intervalSeconds = 60
    @State private var enabled = true
    @State private var selectedServerIds: Set<String> = []
    @State private var saving = false

    /// Common probe intervals, in seconds.
    private static let intervals = [30, 60, 120, 300, 600, 1800, 3600]

    private var isValid: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty
            && !target.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        NavigationStack {
            Form {
                detailsSection
                serversSection
                Section {
                    Toggle(String(localized: "Enabled"), isOn: $enabled)
                }
                if let error = viewModel.actionError {
                    Section {
                        Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline)
                    }
                }
            }
            .navigationTitle(editing == nil ? String(localized: "New Ping Task") : String(localized: "Edit Ping Task"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbarContent }
            .onAppear(perform: prefill)
        }
    }
}

private extension PingTaskFormSheet {
    var detailsSection: some View {
        Section {
            TextField(String(localized: "Name"), text: $name)
            Picker(String(localized: "Type"), selection: $probeType) {
                ForEach(PingProbeType.allCases) { type in
                    Text(type.label).tag(type)
                }
            }
            .pickerStyle(.segmented)
            TextField(probeType.targetPlaceholder, text: $target)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .keyboardType(.URL)
            Picker(String(localized: "Interval"), selection: $intervalSeconds) {
                ForEach(intervalOptions, id: \.self) { secs in
                    Text(intervalLabel(secs)).tag(secs)
                }
            }
        } header: {
            Text(String(localized: "Details"))
        } footer: {
            Text(String(localized: "ICMP pings a host, TCP needs host:port, HTTP needs a full URL."))
        }
    }

    var serversSection: some View {
        Section {
            ForEach(serversViewModel.servers) { server in
                Button {
                    toggle(server.id)
                } label: {
                    HStack {
                        Text(server.name).foregroundStyle(.primary)
                        Spacer()
                        if selectedServerIds.contains(server.id) {
                            Image(systemName: "checkmark").foregroundStyle(Color.brandAccent)
                        }
                    }
                }
            }
        } header: {
            Text(String(localized: "Servers"))
        } footer: {
            Text(selectedServerIds.isEmpty
                ? String(localized: "No servers selected — runs on all capable servers.")
                : String(format: String(localized: "%d selected"), selectedServerIds.count))
        }
    }

    @ToolbarContentBuilder
    var toolbarContent: some ToolbarContent {
        ToolbarItem(placement: .cancellationAction) {
            Button(String(localized: "Cancel")) { dismiss() }
        }
        ToolbarItem(placement: .confirmationAction) {
            if saving {
                ProgressView()
            } else {
                Button(String(localized: "Save")) { Task { await save() } }
                    .disabled(!isValid)
            }
        }
    }

    /// Preset intervals plus the task's current value if it isn't a preset.
    var intervalOptions: [Int] {
        var opts = Self.intervals
        if !opts.contains(intervalSeconds) {
            opts.append(intervalSeconds)
            opts.sort()
        }
        return opts
    }

    func intervalLabel(_ seconds: Int) -> String {
        if seconds % 3600 == 0 { return String(format: String(localized: "%dh"), seconds / 3600) }
        if seconds % 60 == 0 { return String(format: String(localized: "%dm"), seconds / 60) }
        return String(format: String(localized: "%ds"), seconds)
    }

    func toggle(_ id: String) {
        if selectedServerIds.contains(id) {
            selectedServerIds.remove(id)
        } else {
            selectedServerIds.insert(id)
        }
    }

    func prefill() {
        guard let editing else { return }
        name = editing.name
        probeType = editing.probeType
        target = editing.target
        intervalSeconds = editing.interval
        enabled = editing.enabled
        selectedServerIds = Set(editing.serverIds)
    }

    func save() async {
        saving = true
        defer { saving = false }
        let trimmedName = name.trimmingCharacters(in: .whitespaces)
        let trimmedTarget = target.trimmingCharacters(in: .whitespaces)
        let ids = Array(selectedServerIds)
        let failure: String?
        if let editing {
            failure = await viewModel.update(
                id: editing.id,
                UpdatePingTaskRequest(
                    name: trimmedName,
                    probeType: probeType,
                    target: trimmedTarget,
                    interval: intervalSeconds,
                    serverIds: ids,
                    enabled: enabled
                ),
                apiClient: apiClient
            )
        } else {
            failure = await viewModel.create(
                CreatePingTaskRequest(
                    name: trimmedName,
                    probeType: probeType,
                    target: trimmedTarget,
                    interval: intervalSeconds,
                    serverIds: ids,
                    enabled: enabled
                ),
                apiClient: apiClient
            )
        }
        if failure == nil {
            onSaved()
            dismiss()
        }
    }
}
