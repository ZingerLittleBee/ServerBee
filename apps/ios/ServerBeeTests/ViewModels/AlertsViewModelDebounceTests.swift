import XCTest
@testable import ServerBee

/// Verifies that `AlertsViewModel.handleWSAlertEvent` debounces WebSocket
/// bursts into a single (or at most two) `/api/alert-events` fetches instead
/// of racing N concurrent requests.
@MainActor
final class AlertsViewModelDebounceTests: XCTestCase {
    /// Fires a tight burst of 5 WS alert events. Expect exactly one fetch:
    /// every cancellation lands inside the debounce window so only the
    /// trailing task actually performs the network call.
    func test_handleWSAlertEvent_debouncesBurstOfEvents() async {
        let viewModel = AlertsViewModel()
        viewModel.wsRefetchDebounce = .milliseconds(80)

        // No serverUrl → fetchEvents will throw, but fetchEventsCallCount
        // is incremented at entry regardless, which is the metric we care
        // about for debounce coverage.
        let auth = AuthManager()
        auth.serverUrl = nil
        let client = APIClient(authManager: auth)

        for _ in 0..<5 {
            await viewModel.handleWSAlertEvent(apiClient: client)
        }

        await viewModel.awaitPendingRefetchForTesting()

        XCTAssertEqual(
            viewModel.fetchEventsCallCount,
            1,
            "A tight burst of 5 WS events must coalesce into a single fetch via debounce"
        )
    }

    /// Events separated by longer than the debounce window must each trigger
    /// their own fetch — debounce should not coalesce unrelated events.
    func test_handleWSAlertEvent_separatedByDebounceWindow_triggersTwoFetches() async {
        let viewModel = AlertsViewModel()
        viewModel.wsRefetchDebounce = .milliseconds(20)

        let auth = AuthManager()
        auth.serverUrl = nil
        let client = APIClient(authManager: auth)

        await viewModel.handleWSAlertEvent(apiClient: client)
        await viewModel.awaitPendingRefetchForTesting()

        await viewModel.handleWSAlertEvent(apiClient: client)
        await viewModel.awaitPendingRefetchForTesting()

        XCTAssertEqual(
            viewModel.fetchEventsCallCount,
            2,
            "Two events separated by more than the debounce window must each refetch"
        )
    }
}
