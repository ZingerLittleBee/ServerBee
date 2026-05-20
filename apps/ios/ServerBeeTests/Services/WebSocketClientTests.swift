import XCTest
@testable import ServerBee

@MainActor
final class WebSocketClientTests: XCTestCase {
    func test_connect_decodesIncomingFrameAndCallsOnMessage() async throws {
        let fake = FakeWebSocketTransport()
        let client = WebSocketClient(transportFactory: { _, _ in fake })

        let received = expectation(description: "onMessage fired")
        await client.setOnMessage { msg in
            if case .serverOnline(let id) = msg, id == "abc" {
                received.fulfill()
            }
        }

        await client.connect(serverUrl: "https://example.test", accessToken: "tok")
        await fake.enqueueText(#"{"type":"server_online","server_id":"abc"}"#)

        await fulfillment(of: [received], timeout: 2.0)
    }

    func test_connectionState_isConnectingUntilFirstFrame() async throws {
        let fake = FakeWebSocketTransport()
        let client = WebSocketClient(transportFactory: { _, _ in fake })

        await client.connect(serverUrl: "https://example.test", accessToken: "tok")

        let preState = await client.connectionState
        XCTAssertEqual(preState, .connecting)

        let observed = expectation(description: "moved to connected")
        await client.setConnectionStateObserver { state in
            if state == .connected { observed.fulfill() }
        }
        await fake.enqueueText(#"{"type":"server_online","server_id":"x"}"#)

        await fulfillment(of: [observed], timeout: 2.0)
        let postState = await client.connectionState
        XCTAssertEqual(postState, .connected)
    }
}
