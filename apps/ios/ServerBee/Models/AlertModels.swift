import Foundation

enum AlertStatus: String, Codable, Sendable {
    case firing
    case resolved
    case superseded

    var label: String {
        switch self {
        case .firing: String(localized: "Firing")
        case .resolved: String(localized: "Resolved")
        case .superseded: String(localized: "Superseded")
        }
    }
}

/// One row of the alert-events list (`GET /api/alert-events`). Mirrors the
/// server's `AlertEventResponse`. The list carries a complete alert key,
/// rule/server labels, its current status, the relevant timestamp
/// (`event_at`) and the trigger `count`. The richer fields (message, first/last
/// timestamps, rule mode) live on the per-event detail DTO (`MobileAlertDetail`).
struct MobileAlertEvent: Codable, Identifiable, Sendable {
    let ruleId: String
    let ruleName: String
    let serverId: String
    let serverName: String
    let status: AlertStatus
    /// `first_triggered_at` for firing/superseded, `resolved_at` for resolved.
    let eventAt: String
    let resolvedAt: String?
    let count: Int
    var completeAlertKey: String?

    /// Versioned key binds the event dimension and trigger cycle. Older
    /// Servers fall back to the general-dimension composite key.
    var alertKey: String { completeAlertKey ?? "\(ruleId):\(serverId)" }

    /// Composite ID: the same `alertKey` is reused across firing→resolved
    /// transitions, so disambiguate by status + `eventAt` to avoid duplicate
    /// SwiftUI ForEach IDs.
    var id: String { "\(alertKey)#\(status.rawValue)#\(eventAt)" }

    enum CodingKeys: String, CodingKey {
        case ruleId = "rule_id"
        case ruleName = "rule_name"
        case serverId = "server_id"
        case serverName = "server_name"
        case status
        case eventAt = "event_at"
        case resolvedAt = "resolved_at"
        case count
        case completeAlertKey = "alert_key"
    }
}

struct MobileAlertDetail: Codable, Sendable {
    let alertKey: String
    let ruleId: String
    let ruleName: String
    let serverId: String
    let serverName: String
    let status: AlertStatus
    let message: String
    let triggerCount: Int
    let firstTriggeredAt: String
    let resolvedAt: String?
    let ruleEnabled: Bool
    let ruleTriggerMode: String

    enum CodingKeys: String, CodingKey {
        case alertKey = "alert_key"
        case ruleId = "rule_id"
        case ruleName = "rule_name"
        case serverId = "server_id"
        case serverName = "server_name"
        case status
        case message
        case triggerCount = "trigger_count"
        case firstTriggeredAt = "first_triggered_at"
        case resolvedAt = "resolved_at"
        case ruleEnabled = "rule_enabled"
        case ruleTriggerMode = "rule_trigger_mode"
    }
}
