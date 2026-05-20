import XCTest
import SwiftUI
@testable import ServerBee

@MainActor
final class ContentViewInitTests: XCTestCase {
    func testAPIClientIsAvailableImmediately() {
        let auth = AuthManager()
        auth.serverUrl = "https://example.com"
        let view = ContentView(authManager: auth)
        XCTAssertNotNil(view.apiClientForTest, "APIClient must be constructed before body renders")
    }
}
