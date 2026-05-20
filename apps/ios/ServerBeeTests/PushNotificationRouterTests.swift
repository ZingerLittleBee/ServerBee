import XCTest
@testable import ServerBee

@MainActor
final class PushNotificationRouterTests: XCTestCase {
    func test_enqueue_setsPendingDeepLink() {
        let sut = PushNotificationRouter()
        XCTAssertNil(sut.pendingDeepLink)

        sut.enqueue(.serverDetail(serverId: "srv-1"))

        XCTAssertEqual(sut.pendingDeepLink, .serverDetail(serverId: "srv-1"))
    }

    func test_consume_returnsAndClearsPendingDeepLink() {
        let sut = PushNotificationRouter()
        sut.enqueue(.alertDetail(alertKey: "rule-42"))

        let consumed = sut.consume()

        XCTAssertEqual(consumed, .alertDetail(alertKey: "rule-42"))
        XCTAssertNil(sut.pendingDeepLink)
    }

    func test_consume_whenEmpty_returnsNil() {
        let sut = PushNotificationRouter()
        XCTAssertNil(sut.consume())
    }
}
