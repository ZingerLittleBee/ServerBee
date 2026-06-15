import SwiftUI

/// Server detail "Security" section: an event-type summary plus a live,
/// cursor-paginated security event feed. Gated by the parent on the server's
/// SECURITY_EVENTS capability. Live WS pushes from `SecurityFeedStore` are
/// merged ahead of the REST history.
struct ServerSecuritySection: View {
    let serverId: String

    @Environment(\.apiClient) private var apiClient
    @Environment(SecurityFeedStore.self) private var feed
    @State private var viewModel = ServerSecurityViewModel()
    @State private var selected: SecurityEvent?

    /// Live events merged with REST history, de-duplicated by id, newest-first.
    private var mergedEvents: [SecurityEvent] {
        let live = feed.events(forServer: serverId)
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
        .scrollIndicators(.hidden)
        .refreshable {
            await viewModel.reload(serverId: serverId, apiClient: apiClient)
        }
        .task {
            await viewModel.loadIfNeeded(serverId: serverId, apiClient: apiClient)
        }
        .sheet(item: $selected) { event in
            SecurityEventDetailView(event: event) {
                Task { await viewModel.reload(serverId: serverId, apiClient: apiClient) }
            }
        }
    }

    @ViewBuilder
    private var content: some View {
        if viewModel.isLoading && viewModel.events.isEmpty && mergedEvents.isEmpty {
            loadingState
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
                    onLoadMore: { Task { await viewModel.loadMore(serverId: serverId, apiClient: apiClient) } }
                )
            }
        }
    }

    private var loadingState: some View {
        VStack(spacing: 12) {
            ProgressView()
            Text(String(localized: "Loading security events…"))
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 80)
    }

    private var emptyState: some View {
        ContentUnavailableView {
            Label(String(localized: "No security events"), systemImage: "checkmark.shield")
        } description: {
            Text(String(localized: "No suspicious activity has been detected on this server."))
        }
        .padding(.top, 60)
    }

    private func errorState(_ message: String) -> some View {
        ContentUnavailableView {
            Label(String(localized: "Security unavailable"), systemImage: "exclamationmark.shield")
        } description: {
            Text(message)
        } actions: {
            Button(String(localized: "Retry")) {
                Task { await viewModel.reload(serverId: serverId, apiClient: apiClient) }
            }
        }
        .padding(.top, 60)
    }
}
