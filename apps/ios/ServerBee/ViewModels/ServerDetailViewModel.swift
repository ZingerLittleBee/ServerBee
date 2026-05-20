import SwiftUI

@MainActor
@Observable
final class ServerDetailViewModel {
    var server: ServerStatus?
    var records: [MetricRecord] = []
    var isLoading = false

    /// Set the server from the parent list (avoids a separate network fetch).
    func setServer(_ server: ServerStatus) {
        self.server = server
    }

    /// Fetch the server detail individually if not already provided.
    func fetchDetail(serverId: String, apiClient: APIClient) async {
        guard server == nil else { return }
        do {
            server = try await apiClient.get("/api/servers/\(serverId)")
        } catch {
            AppLog.viewModel.error("ServerDetail fetch failed: \(String(describing: error), privacy: .public)")
        }
    }

    /// Fetch historical metric records for the given server and time range.
    func fetchRecords(serverId: String, range: String, apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            records = try await apiClient.get(MetricsHistoryQuery(range: range).path(serverId: serverId))
        } catch {
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
