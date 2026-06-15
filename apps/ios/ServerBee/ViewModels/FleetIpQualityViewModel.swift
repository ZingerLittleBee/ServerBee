import SwiftUI

/// Backs the cross-server IP-quality overview: each server's egress IP
/// reputation snapshot plus a service-id → name map. Server display names are
/// resolved by the view from the live `ServersViewModel`. Read-only / member-
/// accessible (rechecking stays on the per-server detail screen).
@MainActor
@Observable
final class FleetIpQualityViewModel {
    private(set) var servers: [ServerIpQualityData] = []
    private(set) var serviceNames: [String: String] = [:]

    var isLoading = false
    var loadError: String?

    private var hasLoaded = false

    func loadIfNeeded(apiClient: APIClient) async {
        guard !hasLoaded else { return }
        await reload(apiClient: apiClient)
    }

    func reload(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false; hasLoaded = true }
        loadError = nil

        async let overviewResult: [ServerIpQualityData]? = try? apiClient.get("/api/ip-quality/overview")
        async let servicesResult = fetchServices(apiClient: apiClient)
        let (overview, services) = await (overviewResult, servicesResult)

        if !services.isEmpty {
            serviceNames = Dictionary(services.map { ($0.id, $0.name) }, uniquingKeysWith: { a, _ in a })
        }
        if let overview {
            // Surface checked servers (with a snapshot) first.
            servers = overview.sorted { ($0.ipQuality != nil ? 0 : 1, $0.serverId) < ($1.ipQuality != nil ? 0 : 1, $1.serverId) }
        } else {
            servers = []
            loadError = String(localized: "Couldn't load IP quality")
        }
    }

    private func fetchServices(apiClient: APIClient) async -> [UnlockService] {
        do {
            return try await apiClient.get("/api/ip-quality/services")
        } catch {
            return []
        }
    }
}
