import SwiftUI

/// Admin management of command tasks (one-shot + scheduled). Lists tasks with a
/// type badge and (for scheduled) an enable toggle; tap a row for the detail +
/// results. Lives in the Settings admin section. High-risk (remote command
/// execution) — every write is admin-only and confirmed.
struct TasksView: View {
    let isAdmin: Bool

    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = TasksViewModel()
    @State private var showCreate = false

    var body: some View {
        List {
            if let error = viewModel.actionError ?? viewModel.loadError {
                Section {
                    Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline)
                }
            }
            if viewModel.tasks.isEmpty, !viewModel.isLoading {
                Section {
                    Text(String(localized: "No tasks yet.")).foregroundStyle(.secondary)
                }
            }
            ForEach(viewModel.tasks) { task in
                NavigationLink {
                    TaskDetailView(task: task, viewModel: viewModel, isAdmin: isAdmin)
                } label: {
                    row(for: task)
                }
            }
        }
        .overlay {
            if viewModel.isLoading, viewModel.tasks.isEmpty { ProgressView() }
        }
        .navigationTitle(String(localized: "Scheduled Commands"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if isAdmin {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { showCreate = true } label: { Image(systemName: "plus") }
                }
            }
        }
        .task { await viewModel.load(apiClient: apiClient) }
        .refreshable { await viewModel.load(apiClient: apiClient) }
        .sheet(isPresented: $showCreate) {
            TaskFormSheet(editing: nil, viewModel: viewModel) {}
        }
        #if DEBUG
        .task {
            if isAdmin, UITestSupport.autoPresent == "task-create" { showCreate = true }
        }
        #endif
    }

    @ViewBuilder
    private func row(for task: CommandTask) -> some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 3) {
                Text(task.displayName).font(.body).lineLimit(1)
                HStack(spacing: 6) {
                    Text(task.taskType.label)
                        .font(.caption2.weight(.semibold))
                        .padding(.horizontal, 6).padding(.vertical, 2)
                        .background((task.taskType == .scheduled ? Color.brandAccent : Color.secondary).opacity(0.15))
                        .clipShape(Capsule())
                    if task.taskType == .scheduled, let cron = task.cronExpression {
                        Text(cron).font(.caption2.monospaced()).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
            }
            Spacer(minLength: 8)
            if task.taskType == .scheduled, isAdmin {
                Toggle("", isOn: Binding(
                    get: { task.enabled },
                    set: { newValue in Task { await viewModel.setEnabled(task, enabled: newValue, apiClient: apiClient) } }
                ))
                .labelsHidden()
            }
        }
    }
}
