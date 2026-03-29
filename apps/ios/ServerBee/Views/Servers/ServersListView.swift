import SwiftUI

/// The main servers list view, displayed in the Servers tab.
/// Features search, online/offline filter, pull-to-refresh, and navigation to detail.
struct ServersListView: View {
    @Environment(ServersViewModel.self) private var viewModel
    @Environment(\.apiClient) private var apiClient

    var body: some View {
        @Bindable var viewModel = viewModel
        Group {
            if viewModel.isLoading && viewModel.servers.isEmpty {
                loadingView
            } else if viewModel.servers.isEmpty {
                emptyStateView
            } else {
                serversList
            }
        }
        .navigationTitle(String(localized: "Servers"))
        .searchable(
            text: $viewModel.searchQuery,
            prompt: String(localized: "Search servers...")
        )
        .refreshable {
            if let apiClient {
                await viewModel.refresh(apiClient: apiClient)
            }
        }
        .task {
            if viewModel.servers.isEmpty, let apiClient {
                await viewModel.fetchServers(apiClient: apiClient)
            }
        }
    }

    // MARK: - Subviews

    private var loadingView: some View {
        VStack(spacing: 16) {
            ProgressView()
            Text(String(localized: "Loading servers..."))
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var emptyStateView: some View {
        ContentUnavailableView {
            Label(String(localized: "No Servers"), systemImage: "server.rack")
        } description: {
            Text(String(localized: "Connect an agent to your server to start monitoring."))
        }
    }

    private var serversList: some View {
        ScrollView {
            LazyVStack(spacing: 12) {
                // Header with count and filter
                ServerListHeaderView(
                    filter: Bindable(viewModel).onlineFilter,
                    totalCount: viewModel.servers.count,
                    onlineCount: viewModel.onlineCount
                )
                .padding(.horizontal)

                // Server cards
                let filtered = viewModel.filteredServers
                if filtered.isEmpty {
                    noMatchesView
                } else {
                    ForEach(filtered) { server in
                        NavigationLink(value: server) {
                            ServerCardView(server: server)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .padding(.horizontal)
                    }
                }
            }
            .padding(.vertical)
        }
        .background(Color(.systemGroupedBackground))
        .navigationDestination(for: ServerStatus.self) { server in
            ServerDetailView(server: server)
        }
    }

    private var noMatchesView: some View {
        VStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .font(.largeTitle)
                .foregroundStyle(.secondary)
            Text(String(localized: "No matching servers"))
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 60)
    }
}

#Preview {
    NavigationStack {
        ServersListView()
    }
    .environment(AuthManager())
    .environment(ServersViewModel())
}
