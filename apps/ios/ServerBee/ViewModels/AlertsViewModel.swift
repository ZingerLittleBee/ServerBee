import SwiftUI

@MainActor
@Observable
final class AlertsViewModel {
    var events: [MobileAlertEvent] = []
    var isLoading = false
    var isRefreshing = false

    func fetchEvents(limit: Int = 50, apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            let response: ApiResponse<[MobileAlertEvent]> = try await apiClient.get("/api/alert-events?limit=\(limit)")
            events = response.data
        } catch {
            print("[Alerts] Fetch failed: \(error)")
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
