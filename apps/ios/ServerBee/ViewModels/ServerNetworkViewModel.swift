import SwiftUI

/// Backs the server detail "Network" section: probe summary, configured
/// targets, latency/loss history over a selectable range, and recent anomalies.
@MainActor
@Observable
final class ServerNetworkViewModel {
    var summary: NetworkProbeServerSummary?
    var targets: [NetworkProbeTarget] = []
    var records: [ProbeRecordDto] = []
    var anomalies: [NetworkProbeAnomaly] = []

    var range: NetworkRange = .sixHours
    var isLoading = false
    var isLoadingRecords = false
    var loadError: String?

    private var hasLoaded = false

    func loadIfNeeded(serverId: String, apiClient: APIClient) async {
        guard !hasLoaded else { return }
        await reload(serverId: serverId, apiClient: apiClient)
    }

    func reload(serverId: String, apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false; hasLoaded = true }
        loadError = nil

        async let summaryResult = fetch(NetworkProbeServerSummary.self,
                                        "/api/servers/\(serverId)/network-probes/summary", apiClient)
        async let targetsResult = fetchList(NetworkProbeTarget.self,
                                            "/api/servers/\(serverId)/network-probes/targets", apiClient)
        let (s, t) = await (summaryResult, targetsResult)
        summary = s
        targets = t

        await fetchTimeSeries(serverId: serverId, apiClient: apiClient)

        if s == nil && t.isEmpty && records.isEmpty {
            loadError = String(localized: "Couldn't load network data")
        }
    }

    /// Re-fetch only the records + anomalies when the range changes.
    func reloadTimeSeries(serverId: String, apiClient: APIClient) async {
        await fetchTimeSeries(serverId: serverId, apiClient: apiClient)
    }

    private func fetchTimeSeries(serverId: String, apiClient: APIClient) async {
        isLoadingRecords = true
        defer { isLoadingRecords = false }
        let window = range.window()
        async let recordsResult = fetchList(
            ProbeRecordDto.self,
            "/api/servers/\(serverId)/network-probes/records?from=\(window.from)&to=\(window.to)",
            apiClient
        )
        async let anomaliesResult = fetchList(
            NetworkProbeAnomaly.self,
            "/api/servers/\(serverId)/network-probes/anomalies?from=\(window.from)&to=\(window.to)",
            apiClient
        )
        let (r, a) = await (recordsResult, anomaliesResult)
        records = r
        anomalies = a.sorted { $0.timestamp > $1.timestamp }
    }

    // MARK: - Fetch helpers

    private func fetch<T: Decodable & Sendable>(_ type: T.Type, _ path: String, _ apiClient: APIClient) async -> T? {
        do {
            return try await apiClient.get(path)
        } catch {
            AppLog.viewModel.error("Network fetch failed [\(path, privacy: .public)]: \(String(describing: error), privacy: .public)")
            return nil
        }
    }

    private func fetchList<T: Decodable & Sendable>(_ type: T.Type, _ path: String, _ apiClient: APIClient) async -> [T] {
        do {
            return try await apiClient.get(path)
        } catch {
            AppLog.viewModel.error("Network list fetch failed [\(path, privacy: .public)]: \(String(describing: error), privacy: .public)")
            return []
        }
    }
}

/// History ranges for the network section.
enum NetworkRange: String, CaseIterable, Identifiable {
    case oneHour = "1h"
    case sixHours = "6h"
    case oneDay = "24h"
    case sevenDays = "7d"

    var id: String { rawValue }
    var label: String { rawValue }

    private var hours: Int {
        switch self {
        case .oneHour: 1
        case .sixHours: 6
        case .oneDay: 24
        case .sevenDays: 24 * 7
        }
    }

    /// ISO8601 from/to query values for the selected range.
    func window(now: Date = Date()) -> (from: String, to: String) {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        let start = now.addingTimeInterval(-TimeInterval(hours * 3600))
        return (formatter.string(from: start), formatter.string(from: now))
    }
}
