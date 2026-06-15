import SwiftUI

/// Backs the cross-server traffic overview: per-server billing-cycle usage plus
/// a fleet-wide daily in/out series for the chart. Both reads are member-
/// accessible.
@MainActor
@Observable
final class FleetTrafficViewModel {
    private(set) var servers: [ServerTrafficOverview] = []
    private(set) var daily: [DailyTraffic] = []

    var isLoading = false
    var loadError: String?

    private var hasLoaded = false

    /// Total bytes used this cycle across every configured server.
    var totalCycleBytes: Int64 { servers.reduce(0) { $0 + $1.cycleTotal } }

    func loadIfNeeded(apiClient: APIClient) async {
        guard !hasLoaded else { return }
        await reload(apiClient: apiClient)
    }

    func reload(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false; hasLoaded = true }
        loadError = nil

        async let overviewResult: [ServerTrafficOverview]? = try? apiClient.get("/api/traffic/overview")
        async let dailyResult: [DailyTraffic]? = try? apiClient.get("/api/traffic/overview/daily?days=30")
        let (overview, dailySeries) = await (overviewResult, dailyResult)

        if let overview {
            // Heaviest users first.
            servers = overview.sorted { $0.cycleTotal > $1.cycleTotal }
        } else {
            servers = []
            loadError = String(localized: "Couldn't load traffic overview")
        }
        daily = dailySeries ?? []
    }
}
