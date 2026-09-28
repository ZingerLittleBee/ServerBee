import Foundation
import os

/// Abstraction over `URLSessionWebSocketTask` so tests can inject a fake.
protocol WebSocketTransport: Sendable {
    func resume()
    func cancel(with closeCode: URLSessionWebSocketTask.CloseCode, reason: Data?)
    func receive() async throws -> URLSessionWebSocketTask.Message
    func send(_ message: URLSessionWebSocketTask.Message) async throws
    func sendPing() async throws
}

/// Production transport backed by `URLSessionWebSocketTask`.
final class URLSessionWebSocketTransport: WebSocketTransport, @unchecked Sendable {
    private let task: URLSessionWebSocketTask

    init(task: URLSessionWebSocketTask) {
        self.task = task
    }

    func resume() {
        task.resume()
    }

    func cancel(with closeCode: URLSessionWebSocketTask.CloseCode, reason: Data?) {
        task.cancel(with: closeCode, reason: reason)
    }

    func receive() async throws -> URLSessionWebSocketTask.Message {
        try await task.receive()
    }

    func send(_ message: URLSessionWebSocketTask.Message) async throws {
        try await task.send(message)
    }

    func sendPing() async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            task.sendPing(pongReceiveHandler: Self.pongHandler(resuming: continuation))
        }
    }

    /// URLSession can call a pong handler more than once, e.g. with an error
    /// when the task is cancelled while the ping is pending. Resuming a
    /// continuation twice traps, so only the first call counts.
    static func pongHandler(
        resuming continuation: CheckedContinuation<Void, Error>
    ) -> @Sendable (Error?) -> Void {
        let resumed = OSAllocatedUnfairLock(initialState: false)
        return { error in
            let isFirst = resumed.withLock { done -> Bool in
                defer { done = true }
                return !done
            }
            guard isFirst else { return }
            if let error {
                continuation.resume(throwing: error)
            } else {
                continuation.resume()
            }
        }
    }
}

/// Factory invoked by `WebSocketClient` to obtain a transport for a URL/token.
typealias WebSocketTransportFactory = @Sendable (_ url: URL, _ accessToken: String) -> WebSocketTransport

enum DefaultWebSocketTransportFactory {
    static let factory: WebSocketTransportFactory = { url, accessToken in
        var request = URLRequest(url: url)
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        let task = URLSession.shared.webSocketTask(with: request)
        return URLSessionWebSocketTransport(task: task)
    }
}
