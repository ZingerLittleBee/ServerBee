import SwiftUI

@MainActor
@Observable
final class AlertsViewModel {
    var events: [MobileAlertEvent] = []
    var isLoading = false
    var isRefreshing = false
    var errorMessage: String?

    func fetchEvents(limit: Int = 50, apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            events = try await apiClient.get("/api/alert-events?limit=\(limit)")
            errorMessage = nil
        } catch {
            AppLog.viewModel.error("Alerts fetch failed: \(String(describing: error), privacy: .public)")
            errorMessage = String(
                format: String(localized: "Failed to load alerts: %@"),
                error.localizedDescription
            )
        }
    }

    func refresh(apiClient: APIClient) async {
        isRefreshing = true
        await fetchEvents(apiClient: apiClient)
        isRefreshing = false
    }

    /// Called when WebSocket receives an alert_event message -- re-fetch list
    func handleWSAlertEvent(apiClient: APIClient) async {
        await fetchEvents(apiClient: apiClient)
    }
}
