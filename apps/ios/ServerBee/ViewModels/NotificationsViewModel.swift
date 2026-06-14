import SwiftUI

/// Loads and manages the alerting configuration surface: notification channels
/// and alert rules. Both the list and the mutations are admin-only on the
/// server (the whole `notification`/`alert` router sits behind `require_admin`),
/// so the owning view must already gate on `isAdmin`.
@MainActor
@Observable
final class NotificationsViewModel {
    var channels: [NotificationChannel] = []
    var rules: [AlertRule] = []
    var isLoading = false
    var errorMessage: String?
    var hasLoaded = false

    /// Channel ids with an in-flight test request, so rows can show progress
    /// and disable the button without blocking the rest of the list.
    var testingChannelIds: Set<String> = []
    /// Transient per-channel result of the last test ("Sent" / error text).
    var testResults: [String: String] = [:]
    /// Entity ids with an in-flight enable/disable toggle.
    var togglingIds: Set<String> = []

    func load(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            async let channelsTask: [NotificationChannel] = apiClient.get("/api/notifications")
            async let rulesTask: [AlertRule] = apiClient.get("/api/alert-rules")
            channels = try await channelsTask
            rules = try await rulesTask
            errorMessage = nil
            hasLoaded = true
        } catch {
            AppLog.viewModel.error("Notifications load failed: \(String(describing: error), privacy: .public)")
            errorMessage = String(
                format: String(localized: "Failed to load configuration: %@"),
                error.localizedDescription
            )
        }
    }

    // MARK: - Channels

    func toggleChannel(_ channel: NotificationChannel, apiClient: APIClient) async {
        let newValue = !channel.enabled
        togglingIds.insert(channel.id)
        defer { togglingIds.remove(channel.id) }
        do {
            let updated: NotificationChannel = try await apiClient.put(
                "/api/notifications/\(channel.id)",
                body: ToggleEnabledRequest(enabled: newValue)
            )
            if let idx = channels.firstIndex(where: { $0.id == channel.id }) {
                channels[idx] = updated
            }
        } catch {
            errorMessage = String(
                format: String(localized: "Couldn't update channel: %@"),
                error.localizedDescription
            )
        }
    }

    func testChannel(_ channel: NotificationChannel, apiClient: APIClient) async {
        testingChannelIds.insert(channel.id)
        testResults[channel.id] = nil
        defer { testingChannelIds.remove(channel.id) }
        do {
            try await apiClient.postVoid("/api/notifications/\(channel.id)/test")
            testResults[channel.id] = String(localized: "Sent")
        } catch {
            testResults[channel.id] = String(
                format: String(localized: "Failed: %@"),
                error.localizedDescription
            )
        }
    }

    // MARK: - Alert rules

    func toggleRule(_ rule: AlertRule, apiClient: APIClient) async {
        let newValue = !rule.enabled
        togglingIds.insert(rule.id)
        defer { togglingIds.remove(rule.id) }
        do {
            let updated: AlertRule = try await apiClient.put(
                "/api/alert-rules/\(rule.id)",
                body: ToggleEnabledRequest(enabled: newValue)
            )
            if let idx = rules.firstIndex(where: { $0.id == rule.id }) {
                rules[idx] = updated
            }
        } catch {
            errorMessage = String(
                format: String(localized: "Couldn't update rule: %@"),
                error.localizedDescription
            )
        }
    }
}
