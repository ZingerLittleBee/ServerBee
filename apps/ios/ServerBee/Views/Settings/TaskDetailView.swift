import SwiftUI

/// Detail for a command task: summary, run-now (scheduled only), enable toggle,
/// edit, delete, and the per-server execution results history.
struct TaskDetailView: View {
    let task: CommandTask
    @Bindable var viewModel: TasksViewModel
    let isAdmin: Bool

    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss

    @State private var results: [TaskResult] = []
    @State private var loadingResults = false
    @State private var showEdit = false
    @State private var showRunConfirm = false
    @State private var showDeleteConfirm = false
    @State private var running = false

    /// The latest version of this task from the VM (so toggles/runs reflect).
    private var current: CommandTask {
        viewModel.tasks.first { $0.id == task.id } ?? task
    }

    var body: some View {
        List {
            summarySection
            if isAdmin {
                actionsSection
            }
            resultsSection
            if let error = viewModel.actionError {
                Section {
                    Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline)
                }
            }
        }
        .navigationTitle(current.displayName)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if isAdmin {
                ToolbarItem(placement: .topBarTrailing) {
                    Button(String(localized: "Edit")) { showEdit = true }
                }
            }
        }
        .task { await loadResults() }
        .refreshable { await loadResults() }
        .sheet(isPresented: $showEdit) {
            TaskFormSheet(editing: current, viewModel: viewModel) { Task { await loadResults() } }
        }
        .confirmationDialog(String(localized: "Run this task now?"), isPresented: $showRunConfirm, titleVisibility: .visible) {
            Button(String(localized: "Run now")) { Task { await runNow() } }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(localized: "Executes the command on all target servers immediately."))
        }
        .confirmationDialog(String(localized: "Delete this task?"), isPresented: $showDeleteConfirm, titleVisibility: .visible) {
            Button(String(localized: "Delete"), role: .destructive) {
                Task { await viewModel.delete(id: current.id, apiClient: apiClient); dismiss() }
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(localized: "Removes the task and its result history. This cannot be undone."))
        }
    }
}

private extension TaskDetailView {
    var summarySection: some View {
        Section(String(localized: "Command")) {
            Text(current.command)
                .font(.system(.callout, design: .monospaced))
                .textSelection(.enabled)
            DetailRow(label: String(localized: "Type"), value: current.taskType.label)
            if let cron = current.cronExpression, !cron.isEmpty {
                DetailRow(label: String(localized: "Cron"), value: cron, monospaced: true)
            }
            if let next = current.nextRunAt {
                DetailRow(label: String(localized: "Next run"), value: Formatters.formatRelativeTime(next))
            }
            if let last = current.lastRunAt {
                DetailRow(label: String(localized: "Last run"), value: Formatters.formatRelativeTime(last))
            }
            DetailRow(label: String(localized: "Servers"), value: "\(current.serverIds.count)")
        }
    }

    @ViewBuilder
    var actionsSection: some View {
        Section {
            if current.taskType == .scheduled {
                Toggle(String(localized: "Enabled"), isOn: Binding(
                    get: { current.enabled },
                    set: { newValue in Task { await viewModel.setEnabled(current, enabled: newValue, apiClient: apiClient) } }
                ))
                Button {
                    showRunConfirm = true
                } label: {
                    if running { ProgressView() } else { Label(String(localized: "Run now"), systemImage: "play.fill") }
                }
                .disabled(running)
            }
            Button(role: .destructive) { showDeleteConfirm = true } label: {
                Label(String(localized: "Delete task"), systemImage: "trash")
            }
        }
    }

    @ViewBuilder
    var resultsSection: some View {
        Section(String(localized: "Recent results")) {
            if loadingResults, results.isEmpty {
                ProgressView()
            } else if results.isEmpty {
                Text(String(localized: "No results yet.")).foregroundStyle(.secondary)
            } else {
                ForEach(results) { result in
                    resultRow(result)
                }
            }
        }
    }

    func resultRow(_ result: TaskResult) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Image(systemName: result.isSuccess ? "checkmark.circle.fill" : "exclamationmark.circle.fill")
                    .foregroundStyle(result.isSuccess ? Color.serverOnline : Color.serverOffline)
                Text(serverName(result.serverId)).font(.subheadline.weight(.medium))
                Spacer()
                Text(Formatters.formatRelativeTime(result.finishedAt)).font(.caption).foregroundStyle(.secondary)
            }
            Text(result.statusLabel).font(.caption).foregroundStyle(.secondary)
            if !result.output.isEmpty {
                Text(result.output)
                    .font(.system(.caption2, design: .monospaced))
                    .lineLimit(6)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
        }
        .padding(.vertical, 2)
    }

    func serverName(_ id: String) -> String {
        // Best-effort: the detail VM doesn't hold the server list; show a short id.
        String(id.prefix(8))
    }

    func loadResults() async {
        loadingResults = true
        defer { loadingResults = false }
        results = await viewModel.results(id: task.id, apiClient: apiClient)
    }

    func runNow() async {
        running = true
        defer { running = false }
        _ = await viewModel.run(id: current.id, apiClient: apiClient)
        await loadResults()
    }
}
