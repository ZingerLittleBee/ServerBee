import Foundation

/// Alert status reported in `alert_event` WebSocket messages.
enum AlertStatus: String, Codable, Sendable {
    case firing
    case resolved
}
