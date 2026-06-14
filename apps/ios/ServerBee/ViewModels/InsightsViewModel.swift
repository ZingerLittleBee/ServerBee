import SwiftUI

/// Backs the Insights hub: fleet-wide cost, service-monitor status, and
/// operational incidents / maintenance. All reads are member-accessible.
@MainActor
@Observable
final class InsightsViewModel {
    var costOverview: CostOverviewResponse?
    var monitors: [ServiceMonitor] = []
    var incidents: [Incident] = []
    var maintenances: [Maintenance] = []

    var isLoading = false
    var hasLoaded = false

    func load(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false; hasLoaded = true }
        async let cost: CostOverviewResponse? = try? apiClient.get("/api/cost/overview")
        async let mons: [ServiceMonitor]? = try? apiClient.get("/api/service-monitors")
        async let incs: [Incident]? = try? apiClient.get("/api/incidents")
        async let maints: [Maintenance]? = try? apiClient.get("/api/maintenances")

        costOverview = await cost
        monitors = (await mons ?? []).sorted { $0.name < $1.name }
        incidents = (await incs ?? []).sorted { $0.createdAt > $1.createdAt }
        maintenances = (await maints ?? []).sorted { $0.startAt > $1.startAt }
    }

    // MARK: - Derived

    var activeIncidents: [Incident] { incidents.filter { !$0.isResolved } }
    var recentResolved: [Incident] { incidents.filter(\.isResolved).prefix(10).map { $0 } }

    var upcomingMaintenances: [Maintenance] { maintenances.filter(\.active) }

    var monitorsDown: Int { monitors.filter { $0.isUp == false }.count }
    var monitorsUp: Int { monitors.filter { $0.isUp == true }.count }
}
