import SwiftUI

/// Firewall blocklist management: aggregate stats, a cursor-paginated list of
/// blocked targets, and (for admins) create/delete actions. Reachable from
/// Settings and surfaced for the whole fleet (not a single server).
struct FirewallBlocklistView: View {
    @Environment(\.apiClient) private var apiClient
    @Environment(AuthManager.self) private var authManager
    @State private var viewModel = FirewallViewModel()
    @State private var showAdd = false
    @State private var pendingDelete: BlockListItem?

    private var isAdmin: Bool { authManager.user?.role.lowercased() == "admin" }

    var body: some View {
        ScrollView {
            content
                .padding(16)
        }
        .background(Color(.systemGroupedBackground))
        .navigationTitle(String(localized: "Firewall"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if isAdmin {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { showAdd = true } label: { Image(systemName: "plus") }
                }
            }
        }
        .refreshable { await viewModel.reload(apiClient: apiClient) }
        .task {
            await viewModel.loadIfNeeded(apiClient: apiClient)
            #if DEBUG
            if isAdmin, UITestSupport.autoPresent == "addblock" { showAdd = true }
            #endif
        }
        .sheet(isPresented: $showAdd) {
            AddBlockSheet { request in
                let ok = await viewModel.create(request, apiClient: apiClient)
                return ok ? nil : viewModel.actionError
            }
        }
        .confirmationDialog(
            String(localized: "Remove this block?"),
            isPresented: Binding(get: { pendingDelete != nil }, set: { if !$0 { pendingDelete = nil } }),
            titleVisibility: .visible
        ) {
            if let block = pendingDelete {
                Button(String(localized: "Remove \(block.target)"), role: .destructive) {
                    Task { await viewModel.delete(id: block.id, apiClient: apiClient) }
                }
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(localized: "Traffic from this target will be allowed again on the covered servers."))
        }
    }

    @ViewBuilder
    private var content: some View {
        if viewModel.isLoading && viewModel.blocks.isEmpty {
            ProgressView().frame(maxWidth: .infinity).padding(.top, 80)
        } else if let error = viewModel.loadError, viewModel.blocks.isEmpty {
            ContentUnavailableView {
                Label(String(localized: "Blocklist unavailable"), systemImage: "lock.slash")
            } description: {
                Text(error)
            } actions: {
                Button(String(localized: "Retry")) { Task { await viewModel.reload(apiClient: apiClient) } }
            }
            .padding(.top, 60)
        } else {
            VStack(spacing: 16) {
                if let stats = viewModel.stats {
                    FirewallStatsCard(stats: stats)
                }
                if let message = viewModel.actionError {
                    Label(message, systemImage: "exclamationmark.triangle.fill")
                        .font(.subheadline)
                        .foregroundStyle(Color.serverOffline)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12)
                        .background(Color.serverOffline.opacity(0.1))
                        .clipShape(RoundedRectangle(cornerRadius: 10))
                }
                if viewModel.blocks.isEmpty {
                    ContentUnavailableView(
                        String(localized: "No blocks"),
                        systemImage: "checkmark.shield",
                        description: Text(String(localized: "No IPs are currently blocked."))
                    )
                    .padding(.top, 40)
                } else {
                    blockListCard
                }
            }
        }
    }

    private var blockListCard: some View {
        SectionCard(String(localized: "Blocked Targets"), systemImage: "hand.raised") {
            VStack(spacing: 0) {
                ForEach(viewModel.blocks) { block in
                    BlockRow(block: block, canDelete: isAdmin) {
                        pendingDelete = block
                    }
                    if block.id != viewModel.blocks.last?.id {
                        Divider()
                    }
                }
                if viewModel.canLoadMore {
                    Divider()
                    Button {
                        Task { await viewModel.loadMore(apiClient: apiClient) }
                    } label: {
                        HStack {
                            if viewModel.isLoadingMore { ProgressView().controlSize(.small) }
                            Text(viewModel.isLoadingMore ? String(localized: "Loading…") : String(localized: "Load more"))
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 8)
                    }
                    .disabled(viewModel.isLoadingMore)
                }
            }
        }
    }
}

// MARK: - Stats

struct FirewallStatsCard: View {
    let stats: FirewallStats

    var body: some View {
        SectionCard {
            HStack(spacing: 12) {
                kpi("\(stats.total)", String(localized: "Total"), .primary)
                kpi("\(stats.manual)", String(localized: "Manual"), .blue)
                kpi("\(stats.auto)", String(localized: "Auto"), .warningAmber)
                kpi("\(stats.v4)/\(stats.v6)", String(localized: "v4/v6"), .secondary)
            }
        }
    }

    private func kpi(_ value: String, _ label: String, _ color: Color) -> some View {
        VStack(spacing: 4) {
            Text(value)
                .font(.title3.bold().monospacedDigit())
                .foregroundStyle(color)
            Text(label)
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
    }
}

// MARK: - Row

struct BlockRow: View {
    let block: BlockListItem
    let canDelete: Bool
    let onDelete: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: block.isAuto ? "bolt.shield" : "hand.raised.fill")
                .font(.body)
                .foregroundStyle(block.isAuto ? Color.warningAmber : .blue)
                .frame(width: 24)
            VStack(alignment: .leading, spacing: 3) {
                Text(block.target)
                    .font(.subheadline.monospaced())
                HStack(spacing: 6) {
                    Chip(text: block.coverLabel, color: .secondary)
                    Chip(text: block.isAuto ? String(localized: "Auto") : String(localized: "Manual"),
                         color: block.isAuto ? .warningAmber : .blue)
                }
                if let comment = block.comment, !comment.isEmpty {
                    Text(comment)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
            Spacer(minLength: 8)
            if canDelete {
                Button(role: .destructive, action: onDelete) {
                    Image(systemName: "trash")
                        .font(.caption)
                        .foregroundStyle(Color.serverOffline)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.vertical, 8)
    }
}
