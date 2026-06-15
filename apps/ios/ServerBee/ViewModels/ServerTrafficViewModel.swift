import SwiftUI

/// Backs the server detail "Traffic" section. Loads billing-cycle traffic, cost
/// insights and the daily uptime window in parallel. All three endpoints are
/// member-readable, so this drives no admin gating.
@MainActor
@Observable
final class ServerTrafficViewModel {
    var traffic: TrafficResponse?
    var cost: ServerCostInsights?
    var uptime: [UptimeDailyEntry] = []
    var isLoading = false
    var loadError: String?

    /// Window for the uptime timeline. 90 is the server default.
    let uptimeDays = 90

    private var hasLoaded = false

    /// Load all three datasets once. Pull-to-refresh calls `reload`.
    func loadIfNeeded(serverId: String, apiClient: APIClient) async {
        guard !hasLoaded else { return }
        await reload(serverId: serverId, apiClient: apiClient)
    }

    func reload(serverId: String, apiClient: APIClient) async {
        isLoading = true
        defer {
            isLoading = false
            hasLoaded = true
        }
        loadError = nil

        async let trafficResult = fetchTraffic(serverId: serverId, apiClient: apiClient)
        async let costResult = fetchCost(serverId: serverId, apiClient: apiClient)
        async let uptimeResult = fetchUptime(serverId: serverId, apiClient: apiClient)

        let (t, c, u) = await (trafficResult, costResult, uptimeResult)
        traffic = t
        cost = c
        uptime = u

        // Traffic is the primary dataset; surface a transient error only if it
        // failed while we have nothing cached to show.
        if t == nil && c == nil && u.isEmpty {
            loadError = String(localized: "Couldn't load traffic data")
        }
    }

    private func fetchTraffic(serverId: String, apiClient: APIClient) async -> TrafficResponse? {
        do {
            return try await apiClient.get("/api/servers/\(serverId)/traffic")
        } catch {
            AppLog.viewModel.error("Traffic fetch failed: \(String(describing: error), privacy: .public)")
            return nil
        }
    }

    private func fetchCost(serverId: String, apiClient: APIClient) async -> ServerCostInsights? {
        do {
            return try await apiClient.get("/api/servers/\(serverId)/cost-insights")
        } catch {
            AppLog.viewModel.error("Cost insights fetch failed: \(String(describing: error), privacy: .public)")
            return nil
        }
    }

    private func fetchUptime(serverId: String, apiClient: APIClient) async -> [UptimeDailyEntry] {
        do {
            return try await apiClient.get("/api/servers/\(serverId)/uptime-daily?days=\(uptimeDays)")
        } catch {
            AppLog.viewModel.error("Uptime fetch failed: \(String(describing: error), privacy: .public)")
            return []
        }
    }
}
