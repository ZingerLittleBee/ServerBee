import Foundation

/// A service monitor (`GET /api/service-monitors`): an admin-defined uptime
/// check (SSL / DNS / HTTP-keyword / TCP / WHOIS) run on a schedule, distinct
/// from the per-server agent ping probes.
struct ServiceMonitor: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let monitorType: String
    let target: String
    let interval: Int
    let configJson: String
    let notificationGroupId: String?
    let retryCount: Int
    let serverIdsJson: String?
    let enabled: Bool
    let lastStatus: Bool?
    let consecutiveFailures: Int
    let lastCheckedAt: String?
    let createdAt: String
    let updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, name, target, interval, enabled
        case monitorType = "monitor_type"
        case configJson = "config_json"
        case notificationGroupId = "notification_group_id"
        case retryCount = "retry_count"
        case serverIdsJson = "server_ids_json"
        case lastStatus = "last_status"
        case consecutiveFailures = "consecutive_failures"
        case lastCheckedAt = "last_checked_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    /// `true` up, `false` down, `nil` not yet checked.
    var isUp: Bool? { lastStatus }

    var typeLabel: String {
        switch monitorType {
        case "ssl": String(localized: "SSL")
        case "dns": String(localized: "DNS")
        case "http_keyword": String(localized: "HTTP")
        case "tcp": String(localized: "TCP")
        case "whois": String(localized: "WHOIS")
        default: monitorType.uppercased()
        }
    }

    var typeIcon: String {
        switch monitorType {
        case "ssl": "lock.shield"
        case "dns": "globe"
        case "http_keyword": "doc.text.magnifyingglass"
        case "tcp": "cable.connector"
        case "whois": "person.text.rectangle"
        default: "dot.radiowaves.left.and.right"
        }
    }
}

/// A single check result for a monitor (`GET /api/service-monitors/{id}/records`).
struct ServiceMonitorRecord: Decodable, Identifiable, Hashable, Sendable {
    let id: Int64
    let monitorId: String
    let success: Bool
    let latency: Double?
    let detailJson: String
    let error: String?
    let time: String

    enum CodingKeys: String, CodingKey {
        case id, success, latency, error, time
        case monitorId = "monitor_id"
        case detailJson = "detail_json"
    }
}

/// `GET /api/service-monitors/{id}` — monitor fields flattened alongside the
/// latest record. Mirrors the server's `#[serde(flatten)]` shape.
struct MonitorWithRecord: Decodable, Sendable {
    let monitor: ServiceMonitor
    let latestRecord: ServiceMonitorRecord?

    enum CodingKeys: String, CodingKey {
        case latestRecord = "latest_record"
    }

    init(from decoder: Decoder) throws {
        monitor = try ServiceMonitor(from: decoder)
        let container = try decoder.container(keyedBy: CodingKeys.self)
        latestRecord = try container.decodeIfPresent(ServiceMonitorRecord.self, forKey: .latestRecord)
    }
}
