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
    /// Group id → display name, from `/api/server-groups`. Used to render the
    /// list grouped and to show a human group name instead of a raw UUID.
    var groupsByID: [String: String] = [:]
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
                (server.ipv6?.lowercased().contains(query) ?? false) ||
                (server.tags?.contains { $0.lowercased().contains(query) } ?? false)
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

    /// Servers grouped by their resolved group name, ordered groups-first then
    /// the ungrouped bucket. Each section is the already-filtered/sorted set.
    var groupedSections: [(group: String?, servers: [ServerStatus])] {
        let filtered = filteredServers
        let grouped = Dictionary(grouping: filtered) { resolvedGroupName(for: $0) }
        let namedKeys = grouped.keys.compactMap { $0 }.sorted { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }
        var sections: [(String?, [ServerStatus])] = namedKeys.map { ($0, grouped[$0] ?? []) }
        if let ungrouped = grouped[nil], !ungrouped.isEmpty {
            sections.append((nil, ungrouped))
        }
        return sections.map { (group: $0.0, servers: $0.1) }
    }

    /// Whether more than one group bucket exists (drives whether the list
    /// renders grouped sections or a flat list).
    var hasMultipleGroups: Bool {
        let names = Set(servers.map { resolvedGroupName(for: $0) ?? "" })
        return names.count > 1
    }

    /// Human-readable group name for a server, or `nil` if ungrouped.
    func resolvedGroupName(for server: ServerStatus) -> String? {
        if let id = server.groupId, let name = groupsByID[id] { return name }
        // Fall back to a directly-provided group name (legacy / unresolved id).
        return server.groupName
    }

    /// Online servers count for display in header.
    var onlineCount: Int {
        servers.filter(\.isOnline).count
    }

    func fetchServers(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            async let serversReq: [ServerStatus] = apiClient.get("/api/servers")
            async let groupsReq: [ServerGroup] = apiClient.get("/api/server-groups")
            let (config, groups) = try await (serversReq, groupsReq)
            groupsByID = Dictionary(groups.map { ($0.id, $0.name) }, uniquingKeysWith: { a, _ in a })
            applyConfig(config)
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

    /// Overlay REST config onto the (possibly WS-populated) list WITHOUT erasing
    /// live metrics. Config carries fields the WS never sends (ipv4/ipv6,
    /// capabilities, billing); the WS carries fields config never sends (online,
    /// cpu, load). Merging preserves both. Servers only in config are appended.
    func applyConfig(_ config: [ServerStatus]) {
        var byID = Dictionary(servers.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        var order = servers.map(\.id)
        for cfg in config {
            if var existing = byID[cfg.id] {
                existing.merge(from: cfg)
                byID[cfg.id] = existing
            } else {
                byID[cfg.id] = cfg
                order.append(cfg.id)
            }
        }
        servers = order.compactMap { byID[$0] }
    }

    /// Handle WebSocket BrowserMessage -- port from use-servers-ws.ts.
    func handleWSMessage(_ message: BrowserMessage) {
        switch message {
        case .fullSync(let incoming, _):
            // Authoritative live set: overlay each live frame onto the existing
            // entry so REST-only config (ipv4, capabilities, billing) survives,
            // then drop any server no longer present.
            var byID = Dictionary(servers.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            var result: [ServerStatus] = []
            for live in incoming {
                if var existing = byID[live.id] {
                    existing.merge(from: live)
                    result.append(existing)
                } else {
                    result.append(live)
                }
                byID[live.id] = nil
            }
            servers = result

        case .update(let updatedServers):
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

        case let .capabilitiesChanged(serverId, capabilities, agentLocal, effective):
            if let index = servers.firstIndex(where: { $0.id == serverId }) {
                servers[index].capabilities = capabilities
                servers[index].agentLocalCapabilities = agentLocal
                servers[index].effectiveCapabilities = effective
            }

        default:
            break
        }
    }
}
