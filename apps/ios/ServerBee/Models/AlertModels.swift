import Foundation

enum AlertStatus: String, Codable, Sendable {
    case firing
    case resolved
}

/// One row of the alert-events list (`GET /api/alert-events`). Mirrors the
/// server's `AlertEventResponse` exactly — the list DTO carries only the
/// rule/server identity, a firing/resolved status, the relevant timestamp
/// (`event_at`) and the trigger `count`. The richer fields (message, first/last
/// timestamps, rule mode) live on the per-event detail DTO (`MobileAlertDetail`).
struct MobileAlertEvent: Codable, Identifiable, Sendable {
    let ruleId: String
    let ruleName: String
    let serverId: String
    let serverName: String
    let status: AlertStatus
    /// `first_triggered_at` for firing, `resolved_at` for resolved.
    let eventAt: String
    let resolvedAt: String?
    let count: Int

    /// Detail-endpoint key `rule_id:server_id` (the list DTO omits it; the
    /// detail route is `/api/alert-events/{rule_id:server_id}`). Also drives
    /// navigation from the list.
    var alertKey: String { "\(ruleId):\(serverId)" }

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
