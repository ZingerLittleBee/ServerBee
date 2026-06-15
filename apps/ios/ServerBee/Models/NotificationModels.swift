import Foundation

/// A notification channel (`GET /api/notifications`): a delivery target for
/// alerts (webhook / telegram / bark / email / apns).
struct NotificationChannel: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let notifyType: String
    let configJson: String
    let enabled: Bool
    let createdAt: String

    enum CodingKeys: String, CodingKey {
        case id, name, enabled
        case notifyType = "notify_type"
        case configJson = "config_json"
        case createdAt = "created_at"
    }

    var typeLabel: String {
        switch notifyType {
        case "webhook": String(localized: "Webhook")
        case "telegram": String(localized: "Telegram")
        case "bark": String(localized: "Bark")
        case "email": String(localized: "Email")
        case "apns": String(localized: "Push")
        default: notifyType.capitalized
        }
    }

    var typeIcon: String {
        switch notifyType {
        case "webhook": "link"
        case "telegram": "paperplane"
        case "bark": "iphone.radiowaves.left.and.right"
        case "email": "envelope"
        case "apns": "bell.badge"
        default: "bell"
        }
    }
}

/// An alert rule (`GET /api/alert-rules`). The threshold conditions live in
/// `rulesJson`; mobile surfaces the high-level metadata and enable state.
struct AlertRule: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let enabled: Bool
    let triggerMode: String
    let notificationGroupId: String?
    let coverType: String
    let serverIdsJson: String?
    let createdAt: String
    let updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, name, enabled
        case triggerMode = "trigger_mode"
        case notificationGroupId = "notification_group_id"
        case coverType = "cover_type"
        case serverIdsJson = "server_ids_json"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    var coverLabel: String {
        switch coverType {
        case "all": String(localized: "All servers")
        case "include": String(localized: "Selected servers")
        case "exclude": String(localized: "All except selected")
        default: coverType.capitalized
        }
    }
}

/// Minimal partial-update body to toggle an entity's `enabled` flag.
struct ToggleEnabledRequest: Encodable, Sendable {
    let enabled: Bool
}
