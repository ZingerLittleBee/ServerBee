import Foundation

/// Billing-cycle traffic usage for one server (`GET /api/servers/{id}/traffic`).
///
/// All byte counts are absolute `i64` byte values. `usagePercent` and
/// `prediction` are present only when a `trafficLimit` is configured.
struct TrafficResponse: Decodable, Sendable {
    let cycleStart: String          // "YYYY-MM-DD"
    let cycleEnd: String            // "YYYY-MM-DD"
    let bytesIn: Int64
    let bytesOut: Int64
    let bytesTotal: Int64
    var trafficLimit: Int64?
    var trafficLimitType: String?   // "sum" | "up" | "down"
    var usagePercent: Double?       // 0-100+ (can exceed 100)
    var prediction: TrafficPrediction?
    var daily: [DailyTraffic]
    var hourly: [HourlyTraffic]

    enum CodingKeys: String, CodingKey {
        case cycleStart = "cycle_start"
        case cycleEnd = "cycle_end"
        case bytesIn = "bytes_in"
        case bytesOut = "bytes_out"
        case bytesTotal = "bytes_total"
        case trafficLimit = "traffic_limit"
        case trafficLimitType = "traffic_limit_type"
        case usagePercent = "usage_percent"
        case prediction
        case daily
        case hourly
    }

    /// Human label for the configured limit direction.
    var limitTypeLabel: String? {
        switch trafficLimitType {
        case "sum": String(localized: "Total")
        case "up": String(localized: "Upload")
        case "down": String(localized: "Download")
        default: nil
        }
    }

    /// The byte total that counts against the limit, matching `trafficLimitType`.
    var countedBytes: Int64 {
        switch trafficLimitType {
        case "up": bytesOut
        case "down": bytesIn
        default: bytesTotal
        }
    }
}

/// One calendar day's in/out bytes within the billing cycle.
struct DailyTraffic: Decodable, Identifiable, Sendable {
    let date: String                // "YYYY-MM-DD"
    let bytesIn: Int64
    let bytesOut: Int64

    var id: String { date }
    var bytesTotal: Int64 { bytesIn + bytesOut }

    enum CodingKeys: String, CodingKey {
        case date
        case bytesIn = "bytes_in"
        case bytesOut = "bytes_out"
    }
}

/// One hour of today's in/out bytes.
struct HourlyTraffic: Decodable, Identifiable, Sendable {
    let hour: String                // "YYYY-MM-DD HH:MM:SS"
    let bytesIn: Int64
    let bytesOut: Int64

    var id: String { hour }
    var bytesTotal: Int64 { bytesIn + bytesOut }

    enum CodingKeys: String, CodingKey {
        case hour
        case bytesIn = "bytes_in"
        case bytesOut = "bytes_out"
    }
}

/// End-of-cycle projection. Returned only when ≥3 days have elapsed and a
/// traffic limit is set.
struct TrafficPrediction: Decodable, Sendable {
    let estimatedTotal: Int64       // projected bytes by cycle end
    let estimatedPercent: Double    // 0-100+
    let willExceed: Bool

    enum CodingKeys: String, CodingKey {
        case estimatedTotal = "estimated_total"
        case estimatedPercent = "estimated_percent"
        case willExceed = "will_exceed"
    }
}
