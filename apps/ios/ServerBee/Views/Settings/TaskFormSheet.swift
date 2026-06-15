import SwiftUI

/// Create or edit a command task (admin). One-shot tasks run immediately on
/// save; scheduled tasks run on a cron schedule. Because this executes
/// arbitrary commands on agents, the form shows an explicit high-risk notice
/// and the server multi-select requires at least one server.
struct TaskFormSheet: View {
    /// The task being edited, or nil to create a new one.
    let editing: CommandTask?
    @Bindable var viewModel: TasksViewModel
    let onSaved: () -> Void

    @Environment(ServersViewModel.self) private var serversViewModel
    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss

    @State private var command = ""
    @State private var name = ""
    @State private var kind: TaskKind = .oneshot
    @State private var cron = ""
    @State private var timeoutText = ""
    @State private var retryCount = 0
    @State private var retryIntervalText = "60"
    @State private var selectedServerIds: Set<String> = []
    @State private var saving = false

    private var isValid: Bool {
        !command.trimmingCharacters(in: .whitespaces).isEmpty
            && !selectedServerIds.isEmpty
            && (kind == .oneshot || !cron.trimmingCharacters(in: .whitespaces).isEmpty)
    }

    var body: some View {
        NavigationStack {
            Form {
                riskNotice
                commandSection
                scheduleSection
                advancedSection
                serversSection
                if let error = viewModel.actionError {
                    Section {
                        Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline)
                    }
                }
            }
            .navigationTitle(editing == nil ? String(localized: "New Task") : String(localized: "Edit Task"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbarContent }
            .onAppear(perform: prefill)
        }
    }
}

private extension TaskFormSheet {
    var riskNotice: some View {
        Section {
            Label(
                String(localized: "Runs commands on the selected servers with the agent's privileges. One-shot tasks run immediately on save."),
                systemImage: "exclamationmark.shield"
            )
            .font(.caption)
            .foregroundStyle(Color.warningAmber)
        }
    }

    var commandSection: some View {
        Section {
            TextField(String(localized: "Name (optional)"), text: $name)
            TextField(String(localized: "Command"), text: $command, axis: .vertical)
                .lineLimit(2...6)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .font(.system(.body, design: .monospaced))
        } header: {
            Text(String(localized: "Command"))
        }
    }

    @ViewBuilder
    var scheduleSection: some View {
        Section {
            Picker(String(localized: "Type"), selection: $kind) {
                ForEach(TaskKind.allCases) { Text($0.label).tag($0) }
            }
            .pickerStyle(.segmented)
            .disabled(editing != nil) // type is immutable after creation
            if kind == .scheduled {
                TextField(String(localized: "Cron expression"), text: $cron)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .font(.system(.body, design: .monospaced))
            }
        } header: {
            Text(String(localized: "Schedule"))
        } footer: {
            if kind == .scheduled {
                Text(String(localized: "6-field cron: sec min hour day month weekday. Validated on save."))
            }
        }
    }

    var advancedSection: some View {
        Section(String(localized: "Advanced")) {
            TextField(String(localized: "Timeout seconds (optional)"), text: $timeoutText)
                .keyboardType(.numberPad)
            Stepper(String(format: String(localized: "Retries: %d"), retryCount), value: $retryCount, in: 0...10)
            TextField(String(localized: "Retry interval seconds"), text: $retryIntervalText)
                .keyboardType(.numberPad)
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
            Text(String(localized: "Target servers"))
        } footer: {
            Text(selectedServerIds.isEmpty
                ? String(localized: "Select at least one server.")
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
                Button(editing == nil && kind == .oneshot ? String(localized: "Run") : String(localized: "Save")) {
                    Task { await save() }
                }
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
        command = editing.command
        name = editing.name ?? ""
        kind = editing.taskType
        cron = editing.cronExpression ?? ""
        timeoutText = editing.timeout.map(String.init) ?? ""
        retryCount = editing.retryCount
        retryIntervalText = String(editing.retryInterval)
        selectedServerIds = Set(editing.serverIds)
    }

    func save() async {
        saving = true
        defer { saving = false }
        let trimmedName = name.trimmingCharacters(in: .whitespaces)
        let nameValue = trimmedName.isEmpty ? nil : trimmedName
        let cronValue = kind == .scheduled ? cron.trimmingCharacters(in: .whitespaces) : nil
        let timeout = Int(timeoutText.trimmingCharacters(in: .whitespaces))
        let retryInterval = Int(retryIntervalText.trimmingCharacters(in: .whitespaces))
        let ids = Array(selectedServerIds)
        let failure: String?
        if let editing {
            failure = await viewModel.update(
                id: editing.id,
                UpdateTaskRequest(
                    name: nameValue,
                    command: command.trimmingCharacters(in: .whitespaces),
                    serverIds: ids,
                    cronExpression: cronValue,
                    timeout: timeout,
                    retryCount: retryCount,
                    retryInterval: retryInterval
                ),
                apiClient: apiClient
            )
        } else {
            failure = await viewModel.create(
                CreateTaskRequest(
                    command: command.trimmingCharacters(in: .whitespaces),
                    serverIds: ids,
                    timeout: timeout,
                    taskType: kind,
                    name: nameValue,
                    cronExpression: cronValue,
                    retryCount: retryCount,
                    retryInterval: retryInterval
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
