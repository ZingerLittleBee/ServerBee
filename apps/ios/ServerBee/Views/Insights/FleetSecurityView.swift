import SwiftUI

/// Cross-server security overview: fleet-wide event-type summary and a live,
/// cursor-paginated event feed spanning every server. Live WS pushes from
/// `SecurityFeedStore` are merged ahead of the REST history; each row is tagged
/// with its source server.
struct FleetSecurityView: View {
    @Environment(\.apiClient) private var apiClient
    @Environment(ServersViewModel.self) private var serversViewModel
    @Environment(SecurityFeedStore.self) private var feed
    @State private var viewModel = FleetSecurityViewModel()
    @State private var selected: SecurityEvent?

    /// Maps server id → display name for tagging events.
    private var namesById: [String: String] {
        Dictionary(serversViewModel.servers.map { ($0.id, $0.name) }, uniquingKeysWith: { a, _ in a })
    }

    /// Live (all-server) events merged with REST history, de-duplicated, newest-first.
    private var mergedEvents: [SecurityEvent] {
        let live = feed.events
        let liveIDs = Set(live.map(\.id))
        let rest = viewModel.events.map(SecurityEvent.init(dto:)).filter { !liveIDs.contains($0.id) }
        return (live + rest).sorted { ($0.date ?? .distantPast) > ($1.date ?? .distantPast) }
    }

    var body: some View {
        ScrollView {
            content
                .padding(16)
        }
        .background(Color(.systemGroupedBackground))
        .navigationTitle(String(localized: "Security"))
        .navigationBarTitleDisplayMode(.inline)
        .refreshable { await viewModel.reload(apiClient: apiClient) }
        .task { await viewModel.loadIfNeeded(apiClient: apiClient) }
        .sheet(item: $selected) { event in
            SecurityEventDetailView(event: event, serverName: namesById[event.serverId]) {
                Task { await viewModel.reload(apiClient: apiClient) }
            }
        }
    }

    @ViewBuilder
    private var content: some View {
        if viewModel.isLoading && mergedEvents.isEmpty {
            ProgressView().frame(maxWidth: .infinity).padding(.top, 80)
        } else if let error = viewModel.loadError, mergedEvents.isEmpty {
            errorState(error)
        } else if mergedEvents.isEmpty {
            emptyState
        } else {
            VStack(spacing: 16) {
                SecuritySummaryCard(typeCounts: viewModel.typeCounts)
                SecurityFeedCard(
                    events: mergedEvents,
                    onSelect: { selected = $0 },
                    canLoadMore: viewModel.canLoadMore,
                    isLoadingMore: viewModel.isLoadingMore,
                    onLoadMore: { Task { await viewModel.loadMore(apiClient: apiClient) } },
                    serverName: { namesById[$0.serverId] }
                )
            }
        }
    }

    private var emptyState: some View {
        ContentUnavailableView {
            Label(String(localized: "No security events"), systemImage: "checkmark.shield")
        } description: {
            Text(String(localized: "No suspicious activity has been detected across your servers."))
        }
        .padding(.top, 60)
    }

    private func errorState(_ message: String) -> some View {
        ContentUnavailableView {
            Label(String(localized: "Security unavailable"), systemImage: "exclamationmark.shield")
        } description: {
            Text(message)
        } actions: {
            Button(String(localized: "Retry")) { Task { await viewModel.reload(apiClient: apiClient) } }
        }
        .padding(.top, 60)
    }
}
