import XCTest
@testable import ServerBee

/// Verifies that `AlertsViewModel.fetchEvents` surfaces failures via
/// `errorMessage`. Failure path is induced by pointing the `APIClient` at an
/// `AuthManager` with no `serverUrl`, which causes `performRequest` to throw
/// `APIError.noServerUrl`.
@MainActor
final class AlertsViewModelErrorTests: XCTestCase {
    func test_fetchEvents_setsErrorMessage_onFailure() async {
        let viewModel = AlertsViewModel()
        let auth = AuthManager()
        auth.serverUrl = nil
        let client = APIClient(authManager: auth)

        await viewModel.fetchEvents(apiClient: client)

        XCTAssertNotNil(viewModel.errorMessage)
        XCTAssertTrue(viewModel.events.isEmpty)
    }
}
