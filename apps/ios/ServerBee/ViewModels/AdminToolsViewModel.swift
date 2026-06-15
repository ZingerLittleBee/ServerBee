import SwiftUI

/// Backs the admin audit-log screen (offset/limit paginated, optional action
/// filter). Admin-only.
@MainActor
@Observable
final class AuditLogViewModel {
    private(set) var entries: [AuditLogEntry] = []
    var total = 0
    var actions: [String] = []
    var actionFilter: String?
    var isLoading = false
    var isLoadingMore = false
    var loadError: String?

    private let pageSize = 50
    private var offset = 0

    var canLoadMore: Bool { entries.count < total }

    func reload(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        loadError = nil
        offset = 0
        async let optionsTask: AuditLogOptions? = try? apiClient.get("/api/audit-logs/options")
        let page = await fetch(offset: 0, apiClient: apiClient)
        if let opts = await optionsTask { actions = opts.actions }
        if let page {
            entries = page.entries
            total = page.total
            offset = page.entries.count
        } else {
            entries = []
            loadError = String(localized: "Couldn't load audit log")
        }
    }

    func loadMore(apiClient: APIClient) async {
        guard canLoadMore, !isLoadingMore else { return }
        isLoadingMore = true
        defer { isLoadingMore = false }
        if let page = await fetch(offset: offset, apiClient: apiClient) {
            entries.append(contentsOf: page.entries)
            total = page.total
            offset += page.entries.count
        }
    }

    func setActionFilter(_ action: String?, apiClient: APIClient) async {
        actionFilter = action
        await reload(apiClient: apiClient)
    }

    private func fetch(offset: Int, apiClient: APIClient) async -> AuditLogPage? {
        var path = "/api/audit-logs?limit=\(pageSize)&offset=\(offset)"
        if let actionFilter, let enc = actionFilter.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) {
            path += "&action=\(enc)"
        }
        return try? await apiClient.get(path)
    }
}

/// Backs the admin rate-limit screen (active buckets + reset-all). Admin-only.
@MainActor
@Observable
final class RateLimitViewModel {
    var status: RateLimitStatus?
    var isLoading = false
    var loadError: String?
    var actionError: String?

    func load(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        loadError = nil
        do {
            status = try await apiClient.get("/api/admin/rate-limit")
        } catch {
            loadError = String(localized: "Couldn't load rate limits")
        }
    }

    /// Clear all rate-limit buckets.
    func resetAll(apiClient: APIClient) async {
        actionError = nil
        do {
            // Empty body clears all scopes/IPs.
            try await apiClient.postVoid("/api/admin/rate-limit/reset", body: EmptyBody())
            await load(apiClient: apiClient)
        } catch {
            actionError = String(localized: "Couldn't reset rate limits")
        }
    }

    private struct EmptyBody: Encodable, Sendable {}
}

/// Backs the GeoIP / ASN database maintenance screen. Status is member-ok;
/// downloads are admin-only.
@MainActor
@Observable
final class DatabasesViewModel {
    var geoip: DbStatus?
    var asn: DbStatus?
    var isLoading = false
    var downloadingGeoip = false
    var downloadingAsn = false
    var message: String?

    func load(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        async let g: DbStatus? = try? apiClient.get("/api/geoip/status")
        async let a: DbStatus? = try? apiClient.get("/api/asn/status")
        geoip = await g
        asn = await a
    }

    func downloadGeoip(apiClient: APIClient) async {
        downloadingGeoip = true
        defer { downloadingGeoip = false }
        await download(path: "/api/geoip/download", apiClient: apiClient)
        geoip = try? await apiClient.get("/api/geoip/status")
    }

    func downloadAsn(apiClient: APIClient) async {
        downloadingAsn = true
        defer { downloadingAsn = false }
        await download(path: "/api/asn/download", apiClient: apiClient)
        asn = try? await apiClient.get("/api/asn/status")
    }

    private func download(path: String, apiClient: APIClient) async {
        message = nil
        do {
            let result: DbDownloadResult = try await apiClient.post(path)
            message = result.message
        } catch {
            message = AccountSecurityViewModel.message(for: error, fallback: String(localized: "Download failed"))
        }
    }
}
