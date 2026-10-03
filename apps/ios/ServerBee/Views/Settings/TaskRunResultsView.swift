import SwiftUI

/// Push destinations fetch only the authenticated run. Deleted or forbidden
/// targets expose a dismissible fallback without commands from another run.
struct TaskRunResultsView: View {
    let target: TaskRunTarget
    let authManager: AuthManager
    @Environment(\.apiClient) private var apiClient
    @Environment(\.privacyMode) private var privacyMode
    @State private var viewModel = TaskRunResultsViewModel()

    private var isAdmin: Bool { authManager.user?.role.lowercased() == "admin" }

    var body: some View {
        List {
            if !isAdmin || viewModel.unavailable {
                ContentUnavailableView(String(localized: "Task run unavailable"), systemImage: "exclamationmark.circle",
                                       description: Text("The task may have been deleted or your access may have changed."))
            } else if viewModel.isLoading {
                ProgressView()
            } else if viewModel.results.isEmpty {
                Text("No results yet.").foregroundStyle(.secondary)
            } else {
                ForEach(viewModel.results) { result in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(result.statusLabel).font(.headline)
                        Text(String(result.serverId.prefix(8))).font(.caption).foregroundStyle(.secondary)
                        Text(result.output.maskingIPs(privacyMode)).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                    }
                }
            }
        }
        .navigationTitle("Task run results")
        .task(id: isAdmin) { await load() }
        .refreshable { await load() }
    }

    private func load() async {
        await viewModel.load(target: target, apiClient: apiClient, isAdmin: isAdmin)
    }
}
