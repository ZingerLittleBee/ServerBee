import SwiftUI

/// The main servers list view, displayed in the Servers tab.
/// A fleet summary (online, firing alerts, live download) sits above an
/// inset-grouped list with one section per server group. Features search, an
/// online/offline filter menu, pull-to-refresh, and navigation to detail.
struct ServersListView: View {
    @Environment(ServersViewModel.self) private var viewModel
    @Environment(AlertsViewModel.self) private var alertsViewModel
    @Environment(\.apiClient) private var apiClient
    @Environment(AuthManager.self) private var authManager

    /// Rebuilds the live WebSocket. REST carries no online state, so a
    /// pull-to-refresh must also resync the socket to be a real refresh.
    var resyncLive: @MainActor () async -> Void = {}

    @State private var showAddServer = false

    private var isAdmin: Bool {
        authManager.user?.role.lowercased() == "admin"
    }

    var body: some View {
        @Bindable var viewModel = viewModel
        Group {
            if viewModel.isLoading && viewModel.servers.isEmpty {
                loadingView
            } else if let message = viewModel.errorMessage, viewModel.servers.isEmpty {
                errorView(message: message)
            } else if viewModel.servers.isEmpty {
                emptyStateView
            } else {
                serversList
            }
        }
        .navigationTitle(String(localized: "Servers"))
        .searchable(
            text: $viewModel.searchQuery,
            prompt: String(localized: "Search name, IP, tag")
        )
        // Names, IPs and tags are matched literally; don't let the keyboard rewrite them.
        .textInputAutocapitalization(.never)
        .autocorrectionDisabled()
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                filterMenu
            }
            if isAdmin {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        showAddServer = true
                    } label: {
                        Label(String(localized: "Add Server"), systemImage: "plus")
                    }
                }
            }
        }
        .sheet(isPresented: $showAddServer, onDismiss: {
            Task { await viewModel.refresh(apiClient: apiClient) }
        }, content: {
            AddServerSheet()
        })
        .refreshable {
            await refreshAll()
        }
        .task {
            if viewModel.servers.isEmpty {
                await viewModel.fetchServers(apiClient: apiClient)
            }
            #if DEBUG
            if isAdmin, UITestSupport.autoPresent == "addserver" {
                showAddServer = true
            }
            #endif
        }
        .task {
            // The Alerts tab loads events lazily; fetch here too so the firing
            // count in the summary is real on first launch.
            if alertsViewModel.events.isEmpty, !alertsViewModel.isLoading {
                await alertsViewModel.fetchEvents(apiClient: apiClient)
            }
        }
        .task(id: viewModel.searchQuery) {
            try? await Task.sleep(for: .milliseconds(250))
            if Task.isCancelled { return }
            viewModel.debouncedSearchQuery = viewModel.searchQuery
        }
    }
}

// MARK: - Subviews

private extension ServersListView {
    /// Pull-to-refresh: the live socket, servers and alert events (for the
    /// firing count) in parallel.
    func refreshAll() async {
        let servers = viewModel
        let alerts = alertsViewModel
        let client = apiClient
        let resync = resyncLive
        async let liveDone: Void = resync()
        async let serversDone: Void = servers.refresh(apiClient: client)
        async let alertsDone: Void = alerts.refresh(apiClient: client)
        _ = await (liveDone, serversDone, alertsDone)
    }

    /// Firing alert count for the summary, or `nil` while alerts are
    /// unavailable (first load in flight, or the last fetch failed).
    var firingAlertCount: Int? {
        if alertsViewModel.events.isEmpty,
           alertsViewModel.isLoading || alertsViewModel.errorMessage != nil {
            return nil
        }
        return alertsViewModel.events.reduce(0) { $0 + ($1.status == .firing ? 1 : 0) }
    }

    var filterMenu: some View {
        Menu {
            Picker(String(localized: "Filter"), selection: Bindable(viewModel).onlineFilter) {
                ForEach(OnlineFilter.allCases, id: \.self) { filter in
                    Text(filter.displayName).tag(filter)
                }
            }
        } label: {
            Label(
                String(localized: "Filter"),
                systemImage: viewModel.onlineFilter == .all
                    ? "line.3.horizontal.decrease"
                    : "line.3.horizontal.decrease.circle.fill"
            )
        }
        .accessibilityValue(Text(viewModel.onlineFilter.displayName))
    }

    var loadingView: some View {
        VStack(spacing: 16) {
            ProgressView()
            Text(String(localized: "Loading servers..."))
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    func errorView(message: String) -> some View {
        ContentUnavailableView {
            Label(String(localized: "Couldn't load servers"), systemImage: "exclamationmark.triangle")
        } description: {
            Text(message)
        } actions: {
            Button(String(localized: "Try again")) {
                Task {
                    await viewModel.fetchServers(apiClient: apiClient)
                }
            }
            .buttonStyle(.borderedProminent)
        }
    }

    var emptyStateView: some View {
        ContentUnavailableView {
            Label(String(localized: "No Servers"), systemImage: "server.rack")
        } description: {
            Text(String(localized: "Connect an agent to your server to start monitoring."))
        } actions: {
            if isAdmin {
                Button(String(localized: "Add Server")) { showAddServer = true }
                    .buttonStyle(.borderedProminent)
            }
        }
    }

    var serversList: some View {
        let sections = viewModel.groupedSections
        let showsUngroupedHeader = viewModel.hasMultipleGroups
        return List {
            Section {
                ServerListHeaderView(
                    onlineCount: viewModel.onlineCount,
                    totalCount: viewModel.servers.count,
                    firingAlertCount: firingAlertCount,
                    downloadBytesPerSec: viewModel.onlineDownloadBytesPerSec
                )
                .listRowInsets(EdgeInsets())
                .listRowBackground(Color.clear)
                .listRowSeparator(.hidden)
            }

            if sections.isEmpty {
                Section {
                    noMatchesView
                        .listRowBackground(Color.clear)
                        .listRowSeparator(.hidden)
                }
            } else {
                ForEach(sections, id: \.group) { section in
                    Section {
                        ForEach(section.servers) { server in
                            serverRow(server)
                        }
                    } header: {
                        if let group = section.group {
                            Text(verbatim: group)
                        } else if showsUngroupedHeader {
                            Text(String(localized: "No group"))
                        }
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
        .listSectionSpacing(.compact)
    }

    func serverRow(_ server: ServerStatus) -> some View {
        let link = NavigationLink(value: ServerNavigationTarget.detailById(server.id)) {
            ServerCardView(server: server)
                .equatable()
        }
        // The whole row is the tap target, so hide the chevron. The modifier
        // ships with the iOS 26 SDK (Swift 6.2); older toolchains, such as the
        // Xcode 16.4 CI runner, keep the standard disclosure indicator.
        #if compiler(>=6.2)
        return link.navigationLinkIndicatorVisibility(.hidden)
        #else
        return link
        #endif
    }

    /// Empty result for a search and/or the online filter. A filter gets its
    /// own wording and a way back to every server; a plain search gets the
    /// system search empty state.
    @ViewBuilder
    var noMatchesView: some View {
        let query = viewModel.debouncedSearchQuery.trimmingCharacters(in: .whitespaces)
        switch viewModel.onlineFilter {
        case .all:
            ContentUnavailableView.search(text: query)
        case .online, .offline:
            let isOnline = viewModel.onlineFilter == .online
            ContentUnavailableView {
                Label(
                    isOnline ? String(localized: "No online servers") : String(localized: "No offline servers"),
                    systemImage: isOnline ? "wifi.slash" : "checkmark.circle"
                )
            } description: {
                if !query.isEmpty {
                    Text(String(localized: "No servers in this filter match “\(query)”."))
                } else if isOnline {
                    Text(String(localized: "None of your servers are online right now."))
                } else {
                    Text(String(localized: "All of your servers are online."))
                }
            } actions: {
                Button(String(localized: "Show All Servers")) { viewModel.onlineFilter = .all }
                    .buttonStyle(.bordered)
            }
        }
    }
}

#Preview {
    NavigationStack {
        ServersListView()
    }
    .environment(AuthManager())
    .environment(ServersViewModel())
    .environment(AlertsViewModel())
}
