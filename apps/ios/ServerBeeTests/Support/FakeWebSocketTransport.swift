import Foundation
import os
@testable import ServerBee

/// Test double for `WebSocketTransport`. Messages must be enqueued before
/// `receive()` is awaited. `cancel` causes any subsequent or pending
/// `receive` to throw `CancellationError`.
final class FakeWebSocketTransport: WebSocketTransport, @unchecked Sendable {
    private struct State {
        var pending: [URLSessionWebSocketTask.Message] = []
        var continuations: [CheckedContinuation<URLSessionWebSocketTask.Message, Error>] = []
        var isCancelled = false
        var resumed = false
        var pingCount = 0
        var pingError: Error?
        var sentMessages: [URLSessionWebSocketTask.Message] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var resumed: Bool { state.withLock { $0.resumed } }
    var pingCount: Int { state.withLock { $0.pingCount } }
    var sentMessages: [URLSessionWebSocketTask.Message] {
        state.withLock { $0.sentMessages }
    }
    var pingError: Error? {
        get { state.withLock { $0.pingError } }
        set { state.withLock { $0.pingError = newValue } }
    }

    func resume() {
        state.withLock { $0.resumed = true }
    }

    func cancel(with closeCode: URLSessionWebSocketTask.CloseCode, reason: Data?) {
        let waiters = state.withLock { s -> [CheckedContinuation<URLSessionWebSocketTask.Message, Error>] in
            s.isCancelled = true
            let w = s.continuations
            s.continuations = []
            return w
        }
        for c in waiters {
            c.resume(throwing: CancellationError())
        }
    }

    func receive() async throws -> URLSessionWebSocketTask.Message {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<URLSessionWebSocketTask.Message, Error>) in
            let action: ReceiveAction = state.withLock { s in
                if s.isCancelled {
                    return .cancelled
                }
                if !s.pending.isEmpty {
                    let next = s.pending.removeFirst()
                    return .deliver(next)
                }
                s.continuations.append(continuation)
                return .park
            }
            switch action {
            case .cancelled:
                continuation.resume(throwing: CancellationError())
            case .deliver(let msg):
                continuation.resume(returning: msg)
            case .park:
                break
            }
        }
    }

    private enum ReceiveAction {
        case cancelled
        case deliver(URLSessionWebSocketTask.Message)
        case park
    }

    func send(_ message: URLSessionWebSocketTask.Message) async throws {
        state.withLock { $0.sentMessages.append(message) }
    }

    func sendPing() async throws {
        let error: Error? = state.withLock { s in
            s.pingCount += 1
            return s.pingError
        }
        if let error { throw error }
    }

    // MARK: - Test helpers

    func enqueueText(_ text: String) async {
        let waiter: CheckedContinuation<URLSessionWebSocketTask.Message, Error>? = state.withLock { s in
            if !s.continuations.isEmpty {
                return s.continuations.removeFirst()
            }
            s.pending.append(.string(text))
            return nil
        }
        waiter?.resume(returning: .string(text))
    }

    func failNextReceive(with error: Error) async {
        let waiter: CheckedContinuation<URLSessionWebSocketTask.Message, Error>? = state.withLock { s in
            if !s.continuations.isEmpty {
                return s.continuations.removeFirst()
            }
            return nil
        }
        waiter?.resume(throwing: error)
    }
}
