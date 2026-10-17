import SwiftUI

@MainActor
@Observable
final class ServerDetailViewModel {
    /// Full REST configuration (capabilities, billing, kernel, agent version,
    /// enrollment state) — fields the live WebSocket frame never carries.
    var config: ServerConfig?
    var records: [MetricRecord] = []
    var isLoading = false
    var isLoadingConfig = false
    private var recordsFetchGeneration = 0

    /// Fetch the authoritative REST configuration for the detail screen.
    func fetchConfig(serverId: String, apiClient: APIClient) async {
        isLoadingConfig = true
        defer { isLoadingConfig = false }
        do {
            let refreshed: ServerConfig = try await apiClient.get("/api/servers/\(serverId)")
            guard !Task.isCancelled else { return }
            config = refreshed
        } catch {
            AppLog.viewModel.error("ServerConfig fetch failed: \(String(describing: error), privacy: .public)")
        }
    }

    /// Fetch historical metric records for the given server and time range.
    func fetchRecords(serverId: String, range: String, apiClient: APIClient) async {
        guard !Task.isCancelled else { return }
        recordsFetchGeneration += 1
        let generation = recordsFetchGeneration
        isLoading = true
        defer { if generation == recordsFetchGeneration { isLoading = false } }
        do {
            let refreshed: [MetricRecord] = try await apiClient.get(MetricsHistoryQuery(range: range).path(serverId: serverId))
            guard generation == recordsFetchGeneration, !Task.isCancelled else { return }
            records = refreshed
        } catch {
            guard generation == recordsFetchGeneration, !Task.isCancelled else { return }
            AppLog.viewModel.error("ServerDetail records fetch failed: \(String(describing: error), privacy: .public)")
        }
    }
}

struct MetricsHistoryQuery {
    let range: String
    let now: Date

    init(range: String, now: Date = Date()) {
        self.range = range
        self.now = now
    }

    func path(serverId: String) -> String {
        var components = URLComponents()
        components.path = "/api/servers/\(serverId)/records"
        components.queryItems = [
            URLQueryItem(name: "from", value: timestamp(from: startDate)),
            URLQueryItem(name: "to", value: timestamp(from: now)),
            URLQueryItem(name: "interval", value: interval)
        ]

        return components.string ?? "/api/servers/\(serverId)/records"
    }

    private var startDate: Date {
        now.addingTimeInterval(-TimeInterval(hours * 3_600))
    }

    private var hours: Int {
        switch range {
        case "6h": 6
        case "24h": 24
        case "7d": 24 * 7
        default: 1
        }
    }

    private var interval: String {
        range == "7d" ? "hourly" : "raw"
    }

    private func timestamp(from date: Date) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        return formatter.string(from: date)
    }
}
