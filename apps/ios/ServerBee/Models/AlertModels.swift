import Foundation

enum AlertStatus: String, Codable, Sendable {
    case firing
    case resolved
}

struct MobileAlertEvent: Codable, Identifiable, Sendable {
    var id: String { alertKey }
    let alertKey: String
    let ruleId: String
    let ruleName: String
    let serverId: String
    let serverName: String
    let status: AlertStatus
    let message: String
    let triggerCount: Int
    let firstTriggeredAt: String
    let lastNotifiedAt: String
    let resolvedAt: String?
    let updatedAt: String

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
        case lastNotifiedAt = "last_notified_at"
        case resolvedAt = "resolved_at"
        case updatedAt = "updated_at"
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
