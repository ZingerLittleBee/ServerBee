import SwiftUI

/// Backs the fleet-wide security overview: a cursor-paginated event feed across
/// ALL servers (no `server_id` filter) plus fleet event-type stats. Live WS
/// pushes are merged in by the view from the shared `SecurityFeedStore`. Mirrors
/// `ServerSecurityViewModel` but without the per-server scoping.
@MainActor
@Observable
final class FleetSecurityViewModel {
    private(set) var events: [SecurityEventDto] = []
    private(set) var typeCounts: [StatsBucket] = []

    var isLoading = false
    var isLoadingMore = false
    var loadError: String?

    private var nextCursor: String?
    private var hasLoaded = false

    var canLoadMore: Bool { nextCursor != nil }

    func loadIfNeeded(apiClient: APIClient) async {
        guard !hasLoaded else { return }
        await reload(apiClient: apiClient)
    }

    func reload(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false; hasLoaded = true }
        loadError = nil
        nextCursor = nil

        async let pageResult = fetchPage(cursor: nil, apiClient: apiClient)
        async let statsResult = fetchStats(apiClient: apiClient)
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

    func loadMore(apiClient: APIClient) async {
        guard let cursor = nextCursor, !isLoadingMore else { return }
        isLoadingMore = true
        defer { isLoadingMore = false }
        if let page = await fetchPage(cursor: cursor, apiClient: apiClient) {
            let existing = Set(events.map(\.id))
            events.append(contentsOf: page.items.filter { !existing.contains($0.id) })
            nextCursor = page.nextCursor
        }
    }

    private func fetchPage(cursor: String?, apiClient: APIClient) async -> SecurityEventList? {
        var path = "/api/security/events?limit=50"
        if let cursor, let encoded = cursor.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) {
            path += "&cursor=\(encoded)"
        }
        do {
            return try await apiClient.get(path)
        } catch {
            AppLog.viewModel.error("Fleet security events fetch failed: \(String(describing: error), privacy: .public)")
            return nil
        }
    }

    private func fetchStats(apiClient: APIClient) async -> [StatsBucket] {
        do {
            return try await apiClient.get("/api/security/stats?group_by=event_type")
        } catch {
            AppLog.viewModel.error("Fleet security stats fetch failed: \(String(describing: error), privacy: .public)")
            return []
        }
    }
}
