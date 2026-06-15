import SwiftUI

/// Backs the firewall blocklist screen: a cursor-paginated list of blocks, the
/// aggregate stats header, and admin create/delete actions. Reading is
/// member-allowed; mutating is admin-only (the server enforces 403 either way).
@MainActor
@Observable
final class FirewallViewModel {
    private(set) var blocks: [BlockListItem] = []
    var stats: FirewallStats?
    var isLoading = false
    var isLoadingMore = false
    var isMutating = false
    var loadError: String?
    var actionError: String?

    /// Filter: nil = all, "manual"/"auto".
    var originFilter: String?

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
        let (page, s) = await (pageResult, statsResult)

        stats = s
        if let page {
            blocks = page.items
            nextCursor = page.nextCursor
        } else {
            blocks = []
            loadError = String(localized: "Couldn't load blocklist")
        }
    }

    func loadMore(apiClient: APIClient) async {
        guard let cursor = nextCursor, !isLoadingMore else { return }
        isLoadingMore = true
        defer { isLoadingMore = false }
        if let page = await fetchPage(cursor: cursor, apiClient: apiClient) {
            let seen = Set(blocks.map(\.id))
            blocks.append(contentsOf: page.items.filter { !seen.contains($0.id) })
            nextCursor = page.nextCursor
        }
    }

    /// Create a block. Returns true on success.
    @discardableResult
    func create(_ request: CreateBlockRequest, apiClient: APIClient) async -> Bool {
        isMutating = true
        actionError = nil
        defer { isMutating = false }
        do {
            let created: BlockListItem = try await apiClient.post("/api/firewall/blocks", body: request)
            blocks.insert(created, at: 0)
            await refreshStats(apiClient: apiClient)
            return true
        } catch {
            actionError = friendlyMessage(for: error)
            return false
        }
    }

    func delete(id: String, apiClient: APIClient) async {
        isMutating = true
        actionError = nil
        defer { isMutating = false }
        do {
            let _: Bool = try await apiClient.delete("/api/firewall/blocks/\(id)")
            blocks.removeAll { $0.id == id }
            await refreshStats(apiClient: apiClient)
        } catch {
            actionError = friendlyMessage(for: error)
        }
    }

    // MARK: - Fetch helpers

    private func fetchPage(cursor: String?, apiClient: APIClient) async -> BlockListResponse? {
        var path = "/api/firewall/blocks?limit=50"
        if let originFilter { path += "&origin=\(originFilter)" }
        if let cursor, let enc = cursor.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) {
            path += "&cursor=\(enc)"
        }
        do {
            return try await apiClient.get(path)
        } catch {
            AppLog.viewModel.error("Firewall list fetch failed: \(String(describing: error), privacy: .public)")
            return nil
        }
    }

    private func fetchStats(apiClient: APIClient) async -> FirewallStats? {
        try? await apiClient.get("/api/firewall/stats")
    }

    private func refreshStats(apiClient: APIClient) async {
        if let s = await fetchStats(apiClient: apiClient) { stats = s }
    }

    private func friendlyMessage(for error: Error) -> String {
        if case APIError.httpError(let code, let data) = error {
            if code == 403 { return String(localized: "Admin permission required") }
            if let msg = Self.errorMessage(from: data) { return msg }
            if code == 409 { return String(localized: "That target is already blocked") }
            if code == 400 { return String(localized: "Invalid or protected target") }
        }
        return String(localized: "Action failed")
    }

    private static func errorMessage(from data: Data) -> String? {
        guard
            let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let err = obj["error"] as? [String: Any],
            let msg = err["message"] as? String
        else { return nil }
        return msg
    }
}
