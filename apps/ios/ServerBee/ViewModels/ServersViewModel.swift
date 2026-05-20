import SwiftUI

enum OnlineFilter: String, CaseIterable {
    case all
    case online
    case offline

    var displayName: String {
        switch self {
        case .all: String(localized: "All")
        case .online: String(localized: "Online")
        case .offline: String(localized: "Offline")
        }
    }
}

@MainActor
@Observable
final class ServersViewModel {
    var servers: [ServerStatus] = []
    var searchQuery = ""
    var debouncedSearchQuery = ""
    var onlineFilter: OnlineFilter = .all
    var isLoading = false
    var isRefreshing = false
    var errorMessage: String?

    var filteredServers: [ServerStatus] {
        var result = servers

        // Search filter (name, ipv4, ipv6 -- case-insensitive)
        // Uses `debouncedSearchQuery` so list reordering happens after the
        // user stops typing, not on every keystroke.
        if !debouncedSearchQuery.isEmpty {
            let query = debouncedSearchQuery.lowercased()
            result = result.filter { server in
                server.name.lowercased().contains(query) ||
                (server.ipv4?.lowercased().contains(query) ?? false) ||
                (server.ipv6?.lowercased().contains(query) ?? false)
            }
        }

        // Online filter
        switch onlineFilter {
        case .all: break
        case .online: result = result.filter { $0.isOnline }
        case .offline: result = result.filter { !$0.isOnline }
        }

        // Sort: online first, then alphabetical
        result.sort { a, b in
            if a.isOnline != b.isOnline { return a.isOnline && !b.isOnline }
            return a.name.localizedCaseInsensitiveCompare(b.name) == .orderedAscending
        }

        return result
    }

    /// Online servers count for display in header.
    var onlineCount: Int {
        servers.filter(\.isOnline).count
    }

    func fetchServers(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            servers = try await apiClient.get("/api/servers")
            errorMessage = nil
        } catch {
            AppLog.viewModel.error("Servers fetch failed: \(String(describing: error), privacy: .public)")
            errorMessage = String(
                format: String(localized: "Failed to load servers: %@"),
                error.localizedDescription
            )
        }
    }

    func refresh(apiClient: APIClient) async {
        isRefreshing = true
        await fetchServers(apiClient: apiClient)
        isRefreshing = false
    }

    /// Handle WebSocket BrowserMessage -- port from use-servers-ws.ts lines 23-73
    func handleWSMessage(_ message: BrowserMessage) {
        switch message {
        case .fullSync(let newServers):
            servers = newServers

        case .update(let updatedServers):
            // Merge updates by ID
            for update in updatedServers {
                if let index = servers.firstIndex(where: { $0.id == update.id }) {
                    servers[index].merge(from: update)
                }
            }

        case .serverOnline(let serverId):
            if let index = servers.firstIndex(where: { $0.id == serverId }) {
                servers[index].online = true
            }

        case .serverOffline(let serverId):
            if let index = servers.firstIndex(where: { $0.id == serverId }) {
                servers[index].online = false
            }

        default:
            break
        }
    }
}
