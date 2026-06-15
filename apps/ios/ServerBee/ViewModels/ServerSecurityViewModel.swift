import SwiftUI

/// Backs the server detail "Security" section: a cursor-paginated event feed
/// filtered to one server, plus event-type stats for the summary. Live WS
/// pushes are merged in by the view from the shared `SecurityFeedStore`.
@MainActor
@Observable
final class ServerSecurityViewModel {
    private(set) var events: [SecurityEventDto] = []
    private(set) var typeCounts: [StatsBucket] = []

    var isLoading = false
    var isLoadingMore = false
    var loadError: String?

    private var nextCursor: String?
    private var hasLoaded = false

    var canLoadMore: Bool { nextCursor != nil }

    func loadIfNeeded(serverId: String, apiClient: APIClient) async {
        guard !hasLoaded else { return }
        await reload(serverId: serverId, apiClient: apiClient)
    }

    func reload(serverId: String, apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false; hasLoaded = true }
        loadError = nil
        nextCursor = nil

        async let pageResult = fetchPage(serverId: serverId, cursor: nil, apiClient: apiClient)
        async let statsResult = fetchStats(serverId: serverId, apiClient: apiClient)
        let (page, stats) = await (pageResult, statsResult)

        typeCounts = stats
        if let page {
            events = page.items
            nextCursor = page.nextCursor
        } else {
            events = []
            loadError = String(localized: "Couldn't load security events")
        }
    }

    func loadMore(serverId: String, apiClient: APIClient) async {
        guard let cursor = nextCursor, !isLoadingMore else { return }
        isLoadingMore = true
        defer { isLoadingMore = false }
        if let page = await fetchPage(serverId: serverId, cursor: cursor, apiClient: apiClient) {
            // De-dup in case a live event already landed in the REST page.
            let existing = Set(events.map(\.id))
            events.append(contentsOf: page.items.filter { !existing.contains($0.id) })
            nextCursor = page.nextCursor
        }
    }

    private func fetchPage(serverId: String, cursor: String?, apiClient: APIClient) async -> SecurityEventList? {
        var path = "/api/security/events?server_id=\(serverId)&limit=50"
        if let cursor, let encoded = cursor.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) {
            path += "&cursor=\(encoded)"
        }
        do {
            return try await apiClient.get(path)
        } catch {
            AppLog.viewModel.error("Security events fetch failed: \(String(describing: error), privacy: .public)")
            return nil
        }
    }

    private func fetchStats(serverId: String, apiClient: APIClient) async -> [StatsBucket] {
        do {
            return try await apiClient.get("/api/security/stats?server_id=\(serverId)&group_by=event_type")
        } catch {
            AppLog.viewModel.error("Security stats fetch failed: \(String(describing: error), privacy: .public)")
            return []
        }
    }
}
