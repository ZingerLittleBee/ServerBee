import Foundation

/// One day of uptime accounting (`GET /api/servers/{id}/uptime-daily?days=N`).
///
/// The server gap-fills missing dates with zeros and returns entries in
/// ascending date order. `onlineMinutes` is the count of received records that
/// day (≈1 per minute); `downtimeIncidents` counts gaps > 2 min between records.
struct UptimeDailyEntry: Decodable, Identifiable, Sendable {
    let date: String                // "YYYY-MM-DD"
    let totalMinutes: Int
    let onlineMinutes: Int
    let downtimeIncidents: Int

    var id: String { date }

    enum CodingKeys: String, CodingKey {
        case date
        case totalMinutes = "total_minutes"
        case onlineMinutes = "online_minutes"
        case downtimeIncidents = "downtime_incidents"
    }

    /// Uptime ratio in `0...1`. Returns `nil` for days with no expected minutes
    /// (gap-filled future/empty days) so the UI can render them as "no data".
    var ratio: Double? {
        guard totalMinutes > 0 else { return nil }
        return min(1.0, Double(onlineMinutes) / Double(totalMinutes))
    }

    /// Health bucket for colour-coding the timeline.
    var status: UptimeStatus {
        guard let ratio else { return .noData }
        if ratio >= 0.9999 && downtimeIncidents == 0 { return .operational }
        if ratio < 0.95 { return .down }
        return .degraded
    }
}

/// Visual health classification of a single uptime day.
enum UptimeStatus: Sendable {
    case operational
    case degraded
    case down
    case noData
}

extension Array where Element == UptimeDailyEntry {
    /// Aggregate uptime ratio across all days that have expected minutes.
    var overallRatio: Double? {
        let online = reduce(0) { $0 + $1.onlineMinutes }
        let total = reduce(0) { $0 + $1.totalMinutes }
        guard total > 0 else { return nil }
        return Swift.min(1.0, Double(online) / Double(total))
    }

    /// Total counted downtime incidents across the window.
    var totalIncidents: Int {
        reduce(0) { $0 + $1.downtimeIncidents }
    }

    /// Count of days with measurable data.
    var daysWithData: Int {
        filter { $0.totalMinutes > 0 }.count
    }
}
