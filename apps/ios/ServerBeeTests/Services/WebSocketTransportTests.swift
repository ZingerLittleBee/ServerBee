import XCTest
@testable import ServerBee

final class WebSocketTransportTests: XCTestCase {
    func test_fakeTransport_deliversEnqueuedMessages() async throws {
        let fake = FakeWebSocketTransport()
        await fake.enqueueText(#"{"type":"server_online","server_id":"x"}"#)

        fake.resume()
        let frame = try await fake.receive()
        if case .string(let s) = frame {
            XCTAssertTrue(s.contains("server_online"))
        } else {
            XCTFail("expected string frame")
        }
    }

    /// URLSession may report a pending ping twice (pong, then the cancel
    /// error); the second call must not resume the continuation again.
    func test_pongHandler_ignoresRepeatedCalls() async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            let handler = URLSessionWebSocketTransport.pongHandler(resuming: continuation)
            handler(nil)
            handler(URLError(.cancelled))
        }
    }

    func test_fakeTransport_cancelStopsReceive() async {
        let fake = FakeWebSocketTransport()
        fake.resume()
        fake.cancel(with: .goingAway, reason: nil)

        do {
            _ = try await fake.receive()
            XCTFail("expected throw after cancel")
        } catch {
            // expected
        }
    }
}
