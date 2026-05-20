import SwiftUI

@MainActor
@Observable
final class AlertsViewModel {
    var events: [MobileAlertEvent] = []
    var isLoading = false
    var isRefreshing = false
    var errorMessage: String?

    /// Debounce window for WebSocket-driven refetches. A burst of N events
    /// inside this window coalesces into a single `/api/alert-events` request.
    /// Exposed for tests so they don't have to wait the full production delay.
    var wsRefetchDebounce: Duration = .milliseconds(250)

    /// Number of fetch attempts. Counts every entry into `fetchEvents` so
    /// tests can verify debouncing collapses bursts.
    private(set) var fetchEventsCallCount = 0

    /// In-flight debounced refetch task. Cancelled if a new event arrives
    /// inside the debounce window so only the trailing fetch survives.
    private var refetchTask: Task<Void, Never>?

    func fetchEvents(limit: Int = 50, apiClient: APIClient) async {
        fetchEventsCallCount &+= 1
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

    /// Called when WebSocket receives an alert_event message. Debounces a
    /// burst of events into a single `/api/alert-events` request: each new
    /// event cancels the prior pending task and starts a fresh `Task` that
    /// sleeps for `wsRefetchDebounce` before fetching.
    func handleWSAlertEvent(apiClient: APIClient) async {
        refetchTask?.cancel()
        let delay = wsRefetchDebounce
        refetchTask = Task { [weak self] in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled else { return }
            guard let self else { return }
            await self.fetchEvents(apiClient: apiClient)
        }
    }

    /// Test hook: await the currently-pending debounced refetch (if any).
    /// Returns immediately if no refetch is in flight.
    func awaitPendingRefetchForTesting() async {
        await refetchTask?.value
    }
}
