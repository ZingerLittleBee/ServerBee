import Foundation

/// A navigation target derived from a push notification payload.
enum ServerDeepLink: Equatable, Hashable, Sendable {
    /// Navigate to a server detail screen.
    case serverDetail(serverId: String)
    /// Navigate to a specific alert (rule id from APNs custom data).
    case alertDetail(alertKey: String)
}
