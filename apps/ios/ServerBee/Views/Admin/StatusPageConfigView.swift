import SwiftUI

/// Admin configuration of the public status page (`/api/status-page`). A single
/// editable form: visibility toggle, title/description, layout, uptime
/// thresholds, the panels shown publicly, and which servers are exposed.
/// All writes are admin-only (enforced server-side).
struct StatusPageConfigView: View {
    let isAdmin: Bool

    @Environment(\.apiClient) private var apiClient
    @Environment(ServersViewModel.self) private var serversViewModel
    @State private var viewModel = StatusPageViewModel()

    @State private var loaded = false
    @State private var saving = false
    @State private var title = ""
    @State private var description = ""
    @State private var enabled = false
    @State private var layout: StatusPageLayout = .list
    @State private var yellow = 99.0
    @State private var red = 95.0
    @State private var showIpQuality = false
    @State private var showServerDetail = false
    @State private var showNetwork = false
    @State private var showIncidents = false
    @State private var showMaintenance = false
    @State private var selectedServerIds: Set<String> = []

    var body: some View {
        Form {
            if let error = viewModel.actionError ?? viewModel.loadError {
                Section {
                    Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline)
                }
            }
            if loaded {
                visibilitySection
                contentSection
                thresholdsSection
                panelsSection
                serversSection
            }
        }
        .overlay { if !loaded { ProgressView() } }
        .navigationTitle(String(localized: "Status Page"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { toolbarContent }
        .task { await initialLoad() }
        .disabled(!isAdmin)
    }
}

private extension StatusPageConfigView {
    var visibilitySection: some View {
        Section {
            Toggle(String(localized: "Public page enabled"), isOn: $enabled)
        } footer: {
            Text(String(localized: "When off, the public status page is not reachable."))
        }
    }

    var contentSection: some View {
        Section(String(localized: "Content")) {
            TextField(String(localized: "Title"), text: $title)
            TextField(String(localized: "Description"), text: $description, axis: .vertical)
                .lineLimit(1...3)
            Picker(String(localized: "Layout"), selection: $layout) {
                ForEach(StatusPageLayout.allCases) { Text($0.label).tag($0) }
            }
            .pickerStyle(.segmented)
        }
    }

    var thresholdsSection: some View {
        Section {
            Stepper(value: $yellow, in: 0...100, step: 0.5) {
                LabeledContent(String(localized: "Warning below"), value: String(format: "%.1f%%", yellow))
            }
            Stepper(value: $red, in: 0...100, step: 0.5) {
                LabeledContent(String(localized: "Critical below"), value: String(format: "%.1f%%", red))
            }
        } header: {
            Text(String(localized: "Uptime Thresholds"))
        } footer: {
            Text(String(localized: "Uptime at or above warning is healthy; below critical is shown as down."))
        }
    }

    var panelsSection: some View {
        Section(String(localized: "Public Panels")) {
            Toggle(String(localized: "Server detail"), isOn: $showServerDetail)
            Toggle(String(localized: "Network"), isOn: $showNetwork)
            Toggle(String(localized: "IP quality"), isOn: $showIpQuality)
            Toggle(String(localized: "Incidents"), isOn: $showIncidents)
            Toggle(String(localized: "Maintenance"), isOn: $showMaintenance)
        }
    }

    var serversSection: some View {
        Section {
            ForEach(serversViewModel.servers) { server in
                Button { toggleServer(server.id) } label: {
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
            Text(String(localized: "Exposed Servers"))
        } footer: {
            Text(selectedServerIds.isEmpty
                ? String(localized: "No servers selected — the public page lists nothing.")
                : String(format: String(localized: "%d selected"), selectedServerIds.count))
        }
    }

    @ToolbarContentBuilder
    var toolbarContent: some ToolbarContent {
        ToolbarItem(placement: .confirmationAction) {
            if saving {
                ProgressView()
            } else if isAdmin {
                Button(String(localized: "Save")) { Task { await save() } }
                    .disabled(!loaded || title.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }

    func initialLoad() async {
        guard !loaded else { return }
        await viewModel.load(apiClient: apiClient)
        guard let config = viewModel.config else { return }
        title = config.title
        description = config.description ?? ""
        enabled = config.enabled
        layout = StatusPageLayout(rawValue: config.defaultLayout) ?? .list
        yellow = config.uptimeYellowThreshold
        red = config.uptimeRedThreshold
        showIpQuality = config.showIpQuality
        showServerDetail = config.showServerDetail
        showNetwork = config.showNetwork
        showIncidents = config.showIncidents
        showMaintenance = config.showMaintenance
        selectedServerIds = Set(config.serverIds)
        loaded = true
    }

    func toggleServer(_ id: String) {
        if selectedServerIds.contains(id) {
            selectedServerIds.remove(id)
        } else {
            selectedServerIds.insert(id)
        }
    }

    func save() async {
        saving = true
        defer { saving = false }
        let request = UpdateStatusPageRequest(
            title: title.trimmingCharacters(in: .whitespaces),
            description: description,
            serverIds: Array(selectedServerIds),
            enabled: enabled,
            uptimeYellowThreshold: yellow,
            uptimeRedThreshold: red,
            showIpQuality: showIpQuality,
            defaultLayout: layout.rawValue,
            showServerDetail: showServerDetail,
            showNetwork: showNetwork,
            showIncidents: showIncidents,
            showMaintenance: showMaintenance
        )
        _ = await viewModel.save(request, apiClient: apiClient)
    }
}
