import XCTest
import os
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

    func test_reconnectDelay_doublesAcrossFailedAttempts() async throws {
        let transports = OSAllocatedUnfairLock(initialState: [FakeWebSocketTransport]())
        let factory: WebSocketTransportFactory = { _, _ in
            let t = FakeWebSocketTransport()
            transports.withLock { $0.append(t) }
            return t
        }
        let client = WebSocketClient(transportFactory: factory)
        await client.setTokenRefresher { "stale" }
        let delays = DelayRecorder()
        await client.setReconnectDelayHook { delay in
            await delays.record(delay)
        }

        await client.connect(serverUrl: "https://example.test", accessToken: "tok")
        // Force three failed connection attempts in a row, none of which
        // ever receive a frame.
        for i in 0..<3 {
            // Wait for transport to exist
            while transports.withLock({ $0.isEmpty }) {
                try await Task.sleep(nanoseconds: 10_000_000)
            }
            let t = transports.withLock { $0.removeFirst() }
            await t.failNextReceive(with: URLError(.networkConnectionLost))
            // Wait until the hook has been invoked i+1 times before driving
            // the next failure (so the next establishConnection runs first).
            while await delays.values.count < i + 1 {
                try await Task.sleep(nanoseconds: 10_000_000)
            }
        }

        let recorded = await delays.values
        XCTAssertEqual(recorded.count, 3)
        XCTAssertEqual(recorded[0], 1.0, accuracy: 0.5)
        XCTAssertGreaterThanOrEqual(recorded[1], 1.6)  // ~2s with jitter
        XCTAssertGreaterThanOrEqual(recorded[2], 3.2)  // ~4s with jitter
    }
}

actor DelayRecorder {
    private(set) var values: [TimeInterval] = []
    func record(_ d: TimeInterval) { values.append(d) }
}

@MainActor
extension WebSocketClientTests {
    func test_rapidReconnect_doesNotDoubleEstablish() async throws {
        let built = OSAllocatedUnfairLock(initialState: 0)
        let factory: WebSocketTransportFactory = { _, _ in
            built.withLock { $0 += 1 }
            return FakeWebSocketTransport()
        }
        let client = WebSocketClient(transportFactory: factory, pingInterval: 60)

        await client.connect(serverUrl: "https://example.test", accessToken: "tok1")
        // Immediately reconnect with a new token while the prior receive loop
        // is still alive.
        await client.connect(serverUrl: "https://example.test", accessToken: "tok2")

        // Wait a tick for any zombie scheduleReconnect to fire.
        try await Task.sleep(nanoseconds: 200_000_000)
        let final = built.withLock { $0 }
        XCTAssertEqual(final, 2, "expected exactly one transport per connect()")
    }

    func test_secondConnect_replacesTransportWithoutManualClose() async throws {
        let fake1 = FakeWebSocketTransport()
        let fake2 = FakeWebSocketTransport()
        let index = OSAllocatedUnfairLock(initialState: 0)
        let factory: WebSocketTransportFactory = { _, _ in
            let i = index.withLock { val -> Int in
                let cur = val
                val += 1
                return cur
            }
            return i == 0 ? fake1 : fake2
        }
        let client = WebSocketClient(transportFactory: factory, pingInterval: 60)

        await client.connect(serverUrl: "https://example.test", accessToken: "t")
        await fake1.enqueueText(#"{"type":"server_online","server_id":"a"}"#)

        // Simulate ContentView re-entering: no .onDisappear close, just a
        // second connect call — the old transport must have been cancelled.
        await client.connect(serverUrl: "https://example.test", accessToken: "t")
        let final = index.withLock { $0 }
        XCTAssertEqual(final, 2)
    }

    func test_pingFailure_triggersReconnect() async throws {
        let fake = FakeWebSocketTransport()
        fake.pingError = URLError(.timedOut)
        let client = WebSocketClient(
            transportFactory: { _, _ in fake },
            pingInterval: 0.05  // fast for tests
        )

        let observed = expectation(description: "reconnect attempted")
        observed.expectedFulfillmentCount = 1
        await client.setReconnectDelayHook { _ in
            observed.fulfill()
        }

        await client.connect(serverUrl: "https://example.test", accessToken: "tok")
        await fake.enqueueText(#"{"type":"server_online","server_id":"x"}"#)

        await fulfillment(of: [observed], timeout: 5.0)
        XCTAssertGreaterThanOrEqual(fake.pingCount, 1)
    }
}
