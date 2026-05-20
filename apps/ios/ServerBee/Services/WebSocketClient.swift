import Foundation

/// A WebSocket client that connects to the ServerBee server's `/api/ws/servers`
/// endpoint, receives `BrowserMessage` frames, and automatically reconnects
/// with exponential backoff on disconnection.
///
/// Implemented as an `actor` so all mutable state is serialized.
actor WebSocketClient {
    enum ConnectionState: Sendable {
        case connecting
        case connected
        case disconnected
    }

    // MARK: - Public observable state

    private(set) var connectionState: ConnectionState = .disconnected

    // MARK: - Private state

    private var transport: WebSocketTransport?
    private var intentionallyClosed = false
    private var reconnectDelay: TimeInterval = 1.0
    private var receiveTask: Task<Void, Never>?

    private var currentServerUrl: String = ""
    private var currentAccessToken: String = ""

    private var onMessage: (@Sendable (BrowserMessage) -> Void)?
    private var tokenRefresher: (@Sendable () async -> String?)?
    private var connectionStateObserver: (@Sendable (ConnectionState) -> Void)?
    private var reconnectDelayHook: (@Sendable (TimeInterval) async -> Void)?

    private let transportFactory: WebSocketTransportFactory
    private let pingInterval: TimeInterval
    private var pingTask: Task<Void, Never>?

    // MARK: - Constants

    private let minReconnectDelay: TimeInterval = 1.0
    private let maxReconnectDelay: TimeInterval = 30.0
    private let jitterFactor: Double = 0.2

    // MARK: - Init

    init(
        transportFactory: @escaping WebSocketTransportFactory = DefaultWebSocketTransportFactory.factory,
        pingInterval: TimeInterval = 25.0
    ) {
        self.transportFactory = transportFactory
        self.pingInterval = pingInterval
    }

    // MARK: - Configuration

    func setOnMessage(_ handler: (@Sendable (BrowserMessage) -> Void)?) {
        self.onMessage = handler
    }

    func setTokenRefresher(_ refresher: (@Sendable () async -> String?)?) {
        self.tokenRefresher = refresher
    }

    func setConnectionStateObserver(_ observer: (@Sendable (ConnectionState) -> Void)?) {
        self.connectionStateObserver = observer
    }

    func setReconnectDelayHook(_ hook: (@Sendable (TimeInterval) async -> Void)?) {
        self.reconnectDelayHook = hook
    }

    // MARK: - Public API

    /// Open a WebSocket connection. Closes any prior connection first.
    func connect(serverUrl: String, accessToken: String) async {
        await closeInternal()
        intentionallyClosed = false
        reconnectDelay = minReconnectDelay
        currentServerUrl = serverUrl
        currentAccessToken = accessToken
        establishConnection()
    }

    /// Intentionally close the connection. No automatic reconnect will happen.
    func close() async {
        intentionallyClosed = true
        await closeInternal()
    }

    // MARK: - Connection lifecycle

    private func closeInternal() async {
        pingTask?.cancel()
        pingTask = nil
        receiveTask?.cancel()
        transport?.cancel(with: .goingAway, reason: nil)
        if let task = receiveTask {
            _ = await task.value
        }
        receiveTask = nil
        transport = nil
        setState(.disconnected)
    }

    private func establishConnection() {
        guard let url = makeWebSocketURL(from: currentServerUrl) else {
            print("[WS] Invalid URL: \(currentServerUrl)")
            return
        }

        setState(.connecting)

        let newTransport = transportFactory(url, currentAccessToken)
        transport = newTransport
        newTransport.resume()
        // NOTE: state moves to .connected only after first successful receive,
        // and reconnectDelay is reset only then (not here).

        receiveTask = Task { [weak self] in
            await self?.receiveLoop(on: newTransport)
        }
    }

    private func receiveLoop(on transport: WebSocketTransport) async {
        var sawFirstFrame = false
        while !Task.isCancelled {
            do {
                let message = try await transport.receive()
                if !sawFirstFrame {
                    sawFirstFrame = true
                    reconnectDelay = minReconnectDelay
                    setState(.connected)
                    startPingTask(on: transport)
                }
                switch message {
                case .string(let text):
                    if let data = text.data(using: .utf8) {
                        do {
                            let browserMessage = try JSONDecoder.snakeCase.decode(
                                BrowserMessage.self, from: data
                            )
                            onMessage?(browserMessage)
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
                await handleReceiveError()
                return
            }
        }
    }

    private func handleReceiveError() async {
        setState(.disconnected)
        if !intentionallyClosed {
            await scheduleReconnect()
        }
    }

    // MARK: - Heartbeat

    private func startPingTask(on transport: WebSocketTransport) {
        pingTask?.cancel()
        pingTask = Task { [weak self, pingInterval] in
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: UInt64(pingInterval * 1_000_000_000))
                if Task.isCancelled { return }
                guard let self else { return }
                let ok = await self.sendHeartbeat(on: transport)
                if !ok { return }
            }
        }
    }

    private func sendHeartbeat(on transport: WebSocketTransport) async -> Bool {
        guard let current = self.transport,
              (current as AnyObject) === (transport as AnyObject) else { return false }
        do {
            try await transport.sendPing()
            return true
        } catch {
            print("[WS] Heartbeat ping failed: \(error)")
            // Force the receive loop to fail by cancelling the transport;
            // it will trigger scheduleReconnect via handleReceiveError.
            transport.cancel(with: .abnormalClosure, reason: nil)
            return false
        }
    }

    // MARK: - Reconnection with exponential backoff

    private func scheduleReconnect() async {
        guard !intentionallyClosed else { return }

        let jitter = 1.0 + (Double.random(in: -1 ... 1) * jitterFactor)
        let delay = min(reconnectDelay * jitter, maxReconnectDelay)
        await reconnectDelayHook?(delay)

        try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))

        guard !intentionallyClosed, !Task.isCancelled else { return }

        reconnectDelay = min(reconnectDelay * 2, maxReconnectDelay)

        if let refresher = tokenRefresher {
            if let newToken = await refresher() {
                currentAccessToken = newToken
            } else {
                setState(.disconnected)
                return
            }
        }

        establishConnection()
    }

    // MARK: - URL helpers

    private func makeWebSocketURL(from raw: String) -> URL? {
        var wsUrl = raw
        if wsUrl.hasPrefix("https://") {
            wsUrl = "wss://" + wsUrl.dropFirst("https://".count)
        } else if wsUrl.hasPrefix("http://") {
            wsUrl = "ws://" + wsUrl.dropFirst("http://".count)
        }
        if wsUrl.hasSuffix("/") {
            wsUrl = String(wsUrl.dropLast())
        }
        wsUrl += "/api/ws/servers"
        return URL(string: wsUrl)
    }

    // MARK: - State helpers

    private func setState(_ new: ConnectionState) {
        connectionState = new
        connectionStateObserver?(new)
    }
}

extension WebSocketClient.ConnectionState: Equatable {}

