import SwiftUI

/// Admin audit log: paginated entries with an optional action filter.
struct AuditLogView: View {
    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = AuditLogViewModel()

    var body: some View {
        List {
            if !viewModel.actions.isEmpty {
                Section {
                    Picker(String(localized: "Action"), selection: Binding(
                        get: { viewModel.actionFilter ?? "" },
                        set: { newValue in
                            Task { await viewModel.setActionFilter(newValue.isEmpty ? nil : newValue, apiClient: apiClient) }
                        }
                    )) {
                        Text(String(localized: "All actions")).tag("")
                        ForEach(viewModel.actions, id: \.self) { action in
                            Text(action).tag(action)
                        }
                    }
                }
            }

            if let error = viewModel.loadError {
                Section { Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline) }
            }

            Section {
                ForEach(viewModel.entries) { entry in
                    AuditRow(entry: entry)
                }
                if viewModel.canLoadMore {
                    Button {
                        Task { await viewModel.loadMore(apiClient: apiClient) }
                    } label: {
                        HStack {
                            Spacer()
                            if viewModel.isLoadingMore { ProgressView() } else { Text(String(localized: "Load more")) }
                            Spacer()
                        }
                    }
                    .disabled(viewModel.isLoadingMore)
                }
            } header: {
                if viewModel.total > 0 {
                    Text(String(localized: "\(viewModel.entries.count) of \(viewModel.total)"))
                }
            }
        }
        .overlay { if viewModel.isLoading, viewModel.entries.isEmpty { ProgressView() } }
        .navigationTitle(String(localized: "Audit Log"))
        .navigationBarTitleDisplayMode(.inline)
        .task { if viewModel.entries.isEmpty { await viewModel.reload(apiClient: apiClient) } }
        .refreshable { await viewModel.reload(apiClient: apiClient) }
    }
}

private struct AuditRow: View {
    let entry: AuditLogEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(entry.action)
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Text(Formatters.formatRelativeTime(entry.createdAt))
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            if let detail = entry.detail, !detail.isEmpty {
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
            }
            Text(verbatim: "\(entry.ip)")
                .font(.caption2.monospaced())
                .foregroundStyle(.tertiary)
        }
        .padding(.vertical, 2)
    }
}
