import Foundation

/// A WebSocket client that connects to the ServerBee server's `/api/ws/servers`
/// endpoint, receives `BrowserMessage` frames, and automatically reconnects
/// with exponential backoff on disconnection.
@Observable
final class WebSocketClient: @unchecked Sendable {
    enum ConnectionState: Sendable {
        case connecting
        case connected
        case disconnected
    }

    // MARK: - Public state

    private(set) var connectionState: ConnectionState = .disconnected

    /// Called on the main actor whenever a decoded `BrowserMessage` arrives.
    var onMessage: (@Sendable (BrowserMessage) -> Void)?

    /// Called before reconnect to obtain a fresh access token.
    var tokenRefresher: (@Sendable () async -> String?)?

    // MARK: - Private state

    private var webSocketTask: URLSessionWebSocketTask?
    private var intentionallyClosed = false
    private var reconnectDelay: TimeInterval = 1.0
    private var receiveTask: Task<Void, Never>?

    private var currentServerUrl: String = ""
    private var currentAccessToken: String = ""

    // MARK: - Constants

    private let minReconnectDelay: TimeInterval = 1.0
    private let maxReconnectDelay: TimeInterval = 30.0
    private let jitterFactor: Double = 0.2

    // MARK: - Public API

    /// Open a WebSocket connection to the given server.
    /// Calling `connect` while already connected will close the previous
    /// connection first.
    func connect(serverUrl: String, accessToken: String) {
        close()
        intentionallyClosed = false
        reconnectDelay = minReconnectDelay
        currentServerUrl = serverUrl
        currentAccessToken = accessToken
        establishConnection()
    }

    /// Intentionally close the connection. No automatic reconnect will happen.
    func close() {
        intentionallyClosed = true
        receiveTask?.cancel()
        receiveTask = nil
        webSocketTask?.cancel(with: .goingAway, reason: nil)
        webSocketTask = nil
        connectionState = .disconnected
    }

    // MARK: - Connection lifecycle

    private func establishConnection() {
        var wsUrl = currentServerUrl
        if wsUrl.hasPrefix("https://") {
            wsUrl = "wss://" + wsUrl.dropFirst("https://".count)
        } else if wsUrl.hasPrefix("http://") {
            wsUrl = "ws://" + wsUrl.dropFirst("http://".count)
        }
        // Ensure no trailing slash before appending path.
        if wsUrl.hasSuffix("/") {
            wsUrl = String(wsUrl.dropLast())
        }
        wsUrl += "/api/ws/servers"

        guard let url = URL(string: wsUrl) else {
            print("[WS] Invalid URL: \(wsUrl)")
            return
        }

        var request = URLRequest(url: url)
        request.setValue("Bearer \(currentAccessToken)", forHTTPHeaderField: "Authorization")

        connectionState = .connecting

        let task = URLSession.shared.webSocketTask(with: request)
        webSocketTask = task
        task.resume()

        // Optimistic: URLSessionWebSocketTask has no delegate-free onOpen
        // callback, so we mark connected immediately and rely on the receive
        // loop to detect actual failures.
        connectionState = .connected
        reconnectDelay = minReconnectDelay

        receiveTask = Task { [weak self] in
            await self?.receiveLoop()
        }
    }

    // MARK: - Receive loop

    private func receiveLoop() async {
        while !Task.isCancelled {
            guard let task = webSocketTask else { break }
            do {
                let message = try await task.receive()
                switch message {
                case .string(let text):
                    if let data = text.data(using: .utf8) {
                        do {
                            let browserMessage = try JSONDecoder.snakeCase.decode(
                                BrowserMessage.self, from: data
                            )
                            await MainActor.run { [weak self] in
                                self?.onMessage?(browserMessage)
                            }
                        } catch {
                            print("[WS] Failed to decode message: \(error)")
                        }
                    }
                case .data:
                    break
                @unknown default:
                    break
                }
            } catch {
                await MainActor.run { [weak self] in
                    self?.connectionState = .disconnected
                }
                if !intentionallyClosed {
                    await scheduleReconnect()
                }
                break
            }
        }
    }

    // MARK: - Reconnection with exponential backoff

    private func scheduleReconnect() async {
        guard !intentionallyClosed else { return }

        let jitter = 1.0 + (Double.random(in: -1 ... 1) * jitterFactor)
        let delay = min(reconnectDelay * jitter, maxReconnectDelay)

        try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))

        guard !intentionallyClosed, !Task.isCancelled else { return }

        reconnectDelay = min(reconnectDelay * 2, maxReconnectDelay)

        // Refresh token before reconnecting
        if let refresher = tokenRefresher {
            if let newToken = await refresher() {
                currentAccessToken = newToken
            } else {
                // Refresh failed — stop reconnecting
                await MainActor.run { [weak self] in
                    self?.connectionState = .disconnected
                }
                return
            }
        }

        await MainActor.run { [weak self] in
            self?.establishConnection()
        }
    }
}
