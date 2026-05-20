import XCTest
@testable import ServerBee

/// Verifies that `filteredServers` keys off `debouncedSearchQuery`, not the
/// live `searchQuery`. The view layer is responsible for copying the live
/// query into the debounced field after a 250ms idle window.
@MainActor
final class ServersViewModelSearchTests: XCTestCase {
    func test_filteredServers_usesDebouncedSearchQuery() {
        let viewModel = ServersViewModel()
        viewModel.servers = [
            ServerStatus(id: "1", name: "alpha", online: true),
            ServerStatus(id: "2", name: "bravo", online: true),
        ]

        viewModel.searchQuery = "alp"
        // Filtering ignores the live searchQuery until the debounced value is updated.
        XCTAssertEqual(viewModel.filteredServers.count, 2)

        viewModel.debouncedSearchQuery = "alp"
        XCTAssertEqual(viewModel.filteredServers.map(\.name), ["alpha"])
    }
}
