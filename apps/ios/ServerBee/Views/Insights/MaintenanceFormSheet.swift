import SwiftUI

/// Create or edit a scheduled maintenance window (admin). Presented from the
/// status / incidents screen. Mirrors the web MaintenanceFormDialog: title,
/// description, start/end window, public toggle, and an optional server scope
/// (no servers selected = applies to all).
struct MaintenanceFormSheet: View {
    /// The window being edited, or nil to create a new one.
    let editing: Maintenance?
    @Bindable var actions: MaintenanceActionsViewModel
    let onSaved: () -> Void

    @Environment(ServersViewModel.self) private var serversViewModel
    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss

    @State private var title = ""
    @State private var description = ""
    @State private var startDate = Date()
    @State private var endDate = Date().addingTimeInterval(3600)
    @State private var isPublic = false
    @State private var selectedServerIds: Set<String> = []

    private var isValid: Bool {
        !title.trimmingCharacters(in: .whitespaces).isEmpty && endDate > startDate
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(String(localized: "Title"), text: $title)
                    TextField(String(localized: "Description (optional)"), text: $description, axis: .vertical)
                        .lineLimit(2...5)
                }
                Section {
                    DatePicker(String(localized: "Starts"), selection: $startDate)
                    DatePicker(String(localized: "Ends"), selection: $endDate)
                    if endDate <= startDate {
                        Label(String(localized: "End must be after start"), systemImage: "exclamationmark.triangle")
                            .font(.caption)
                            .foregroundStyle(Color.warningAmber)
                    }
                } header: {
                    Text(String(localized: "Window"))
                }
                serversSection
                Section {
                    Toggle(String(localized: "Show on public status page"), isOn: $isPublic)
                }
                if let error = actions.errorMessage {
                    Section {
                        Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline)
                    }
                }
            }
            .navigationTitle(editing == nil ? String(localized: "New Maintenance") : String(localized: "Edit Maintenance"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbarContent }
            .onAppear(perform: prefill)
        }
    }
}

private extension MaintenanceFormSheet {
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
            Text(String(localized: "Affected servers"))
        } footer: {
            Text(selectedServerIds.isEmpty
                ? String(localized: "No servers selected — applies to all servers.")
                : String(format: String(localized: "%d selected"), selectedServerIds.count))
        }
    }

    @ToolbarContentBuilder
    var toolbarContent: some ToolbarContent {
        ToolbarItem(placement: .cancellationAction) {
            Button(String(localized: "Cancel")) { dismiss() }
        }
        ToolbarItem(placement: .confirmationAction) {
            if actions.isWorking {
                ProgressView()
            } else {
                Button(String(localized: "Save")) { Task { await save() } }
                    .disabled(!isValid)
            }
        }
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
        title = editing.title
        description = editing.description ?? ""
        if let start = ISO8601DateFormatter.shared.date(from: editing.startAt) { startDate = start }
        if let end = ISO8601DateFormatter.shared.date(from: editing.endAt) { endDate = end }
        isPublic = editing.isPublic
        selectedServerIds = Set(editing.serverIds)
    }

    func save() async {
        let trimmedTitle = title.trimmingCharacters(in: .whitespaces)
        let desc = description.trimmingCharacters(in: .whitespaces)
        let ids: [String]? = selectedServerIds.isEmpty ? nil : Array(selectedServerIds)
        let ok: Bool
        if let editing {
            ok = await actions.update(
                id: editing.id,
                UpdateMaintenanceRequest(
                    title: trimmedTitle,
                    description: desc.isEmpty ? nil : desc,
                    startAt: WireDate.string(from: startDate),
                    endAt: WireDate.string(from: endDate),
                    serverIdsJson: ids,
                    isPublic: isPublic
                ),
                apiClient: apiClient
            )
        } else {
            ok = await actions.create(
                CreateMaintenanceRequest(
                    title: trimmedTitle,
                    description: desc.isEmpty ? nil : desc,
                    startAt: WireDate.string(from: startDate),
                    endAt: WireDate.string(from: endDate),
                    serverIdsJson: ids,
                    isPublic: isPublic
                ),
                apiClient: apiClient
            )
        }
        if ok {
            onSaved()
            dismiss()
        }
    }
}
