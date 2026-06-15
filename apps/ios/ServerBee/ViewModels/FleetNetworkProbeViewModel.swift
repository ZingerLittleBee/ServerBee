import SwiftUI

/// Backs the cross-server network-probe overview: per-server probe health with
/// latest target values and 24h sparklines. Read-only / member-accessible.
@MainActor
@Observable
final class FleetNetworkProbeViewModel {
    private(set) var servers: [NetworkProbeFleetOverview] = []

    var isLoading = false
    var loadError: String?

    private var hasLoaded = false

    /// Servers with at least one anomaly in the last 24h.
    var anomalyServerCount: Int { servers.filter { $0.anomalyCount > 0 }.count }

    func loadIfNeeded(apiClient: APIClient) async {
        guard !hasLoaded else { return }
        await reload(apiClient: apiClient)
    }

    func reload(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false; hasLoaded = true }
        loadError = nil
        do {
            let overview: [NetworkProbeFleetOverview] = try await apiClient.get("/api/network-probes/overview")
            // Anomalous servers first, then offline, then by name.
            servers = overview.sorted {
                ($0.anomalyCount > 0 ? 0 : 1, $0.online ? 1 : 0, $0.serverName)
                    < ($1.anomalyCount > 0 ? 0 : 1, $1.online ? 1 : 0, $1.serverName)
            }
        } catch {
            servers = []
            loadError = String(localized: "Couldn't load network probes")
        }
    }
}
