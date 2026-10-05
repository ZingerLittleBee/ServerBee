import SwiftUI

/// Backs the Insights hub: fleet-wide cost, service-monitor status,
/// operational incidents / maintenance, and the fleet security event total.
/// All reads are member-accessible.
@MainActor
@Observable
final class InsightsViewModel {
    var costOverview: CostOverviewResponse?
    var monitors: [ServiceMonitor] = []
    var incidents: [Incident] = []
    var maintenances: [Maintenance] = []
    /// Fleet-wide security event total (sum of the per-type stats buckets), or
    /// `nil` when the stats request failed.
    var securityEventCount: Int?

    var isLoading = false
    var hasLoaded = false

    func refreshCost(apiClient: APIClient) async {
        let refreshed: CostOverviewResponse? = try? await apiClient.get("/api/cost/overview")
        guard !Task.isCancelled else { return }
        costOverview = refreshed
    }

    func load(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false; hasLoaded = true }
        async let cost: CostOverviewResponse? = try? apiClient.get("/api/cost/overview")
        async let mons: [ServiceMonitor]? = try? apiClient.get("/api/service-monitors")
        async let incs: [Incident]? = try? apiClient.get("/api/incidents")
        async let maints: [Maintenance]? = try? apiClient.get("/api/maintenances")
        async let securityStats: [StatsBucket]? = try? apiClient.get("/api/security/stats?group_by=event_type")

        let (overview, loadedMonitors, loadedIncidents, loadedMaintenances, stats) = await (cost, mons, incs, maints, securityStats)
        guard !Task.isCancelled else { return }
        costOverview = overview
        monitors = (loadedMonitors ?? []).sorted { $0.name < $1.name }
        incidents = (loadedIncidents ?? []).sorted { $0.createdAt > $1.createdAt }
        maintenances = (loadedMaintenances ?? []).sorted { $0.startAt > $1.startAt }
        securityEventCount = stats.map { buckets in buckets.reduce(0) { $0 + $1.count } }
    }

    // MARK: - Derived

    var activeIncidents: [Incident] { incidents.filter { !$0.isResolved } }
    var recentResolved: [Incident] { incidents.filter(\.isResolved).prefix(10).map { $0 } }

    var upcomingMaintenances: [Maintenance] { maintenances.filter(\.active) }

    var monitorsDown: Int { monitors.filter { $0.isUp == false }.count }
    var monitorsUp: Int { monitors.filter { $0.isUp == true }.count }
}
