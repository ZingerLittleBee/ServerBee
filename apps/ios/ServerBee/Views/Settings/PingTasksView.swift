import SwiftUI

/// Admin management of ping tasks: list with per-task enable toggle, swipe to
/// edit/delete, and a create sheet. Lives in the Settings admin section.
struct PingTasksView: View {
    let isAdmin: Bool

    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = PingTasksViewModel()
    @State private var showCreate = false
    @State private var editTarget: PingTask?
    @State private var pendingDelete: PingTask?

    var body: some View {
        List {
            if let error = viewModel.actionError ?? viewModel.loadError {
                Section {
                    Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline)
                }
            }
            if viewModel.tasks.isEmpty, !viewModel.isLoading {
                Section {
                    Text(String(localized: "No ping tasks yet.")).foregroundStyle(.secondary)
                }
            }
            ForEach(viewModel.tasks) { task in
                row(for: task)
            }
        }
        .overlay {
            if viewModel.isLoading, viewModel.tasks.isEmpty { ProgressView() }
        }
        .navigationTitle(String(localized: "Ping Tasks"))
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
            PingTaskFormSheet(editing: nil, viewModel: viewModel) {}
        }
        .sheet(item: $editTarget) { task in
            PingTaskFormSheet(editing: task, viewModel: viewModel) {}
        }
        .confirmationDialog(
            String(localized: "Delete this ping task?"),
            isPresented: Binding(get: { pendingDelete != nil }, set: { if !$0 { pendingDelete = nil } }),
            titleVisibility: .visible
        ) {
            if let task = pendingDelete {
                Button(String(format: String(localized: "Delete %@"), task.name), role: .destructive) {
                    Task { await viewModel.delete(id: task.id, apiClient: apiClient) }
                }
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        }
        #if DEBUG
        .task {
            if isAdmin, UITestSupport.autoPresent == "ping-task-create" { showCreate = true }
        }
        #endif
    }

    @ViewBuilder
    private func row(for task: PingTask) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(task.name).font(.body)
                HStack(spacing: 6) {
                    Text(task.probeType.label)
                        .font(.caption2.weight(.semibold))
                        .padding(.horizontal, 6).padding(.vertical, 2)
                        .background(Color.networkColor.opacity(0.15))
                        .clipShape(Capsule())
                    Text(task.target).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            Spacer(minLength: 8)
            if isAdmin {
                Toggle("", isOn: Binding(
                    get: { task.enabled },
                    set: { newValue in Task { await viewModel.setEnabled(task, enabled: newValue, apiClient: apiClient) } }
                ))
                .labelsHidden()
            } else if task.enabled {
                Image(systemName: "checkmark.circle.fill").foregroundStyle(Color.serverOnline)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture { if isAdmin { editTarget = task } }
        .swipeActions {
            if isAdmin {
                Button(role: .destructive) { pendingDelete = task } label: {
                    Label(String(localized: "Delete"), systemImage: "trash")
                }
                Button { editTarget = task } label: {
                    Label(String(localized: "Edit"), systemImage: "pencil")
                }
                .tint(.brandAccent)
            }
        }
    }
}
