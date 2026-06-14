import Foundation

/// `{"type":"subscribe","container_id":…,"tail":…,"follow":…}` sent to the
/// docker-logs WebSocket once the server signals it is ready.
private struct DockerLogSubscribePayload: Encodable {
    let type = "subscribe"
    let containerId: String
    let tail: Int
    let follow: Bool

    enum CodingKeys: String, CodingKey {
        case type, tail, follow
        case containerId = "container_id"
    }
}

/// A decoded message from the docker-logs WebSocket (`/api/ws/docker/logs/{id}`).
enum DockerLogServerMessage: Equatable, Sendable {
    /// `{"type":"session","session_id":"…"}` — the server is ready; subscribe now.
    case session
    /// `{"type":"logs","entries":[…]}` — a batch of log lines.
    case logs([DockerLogEntry])
    case unknown
}

/// Streams a container's logs over a dedicated WebSocket and accumulates them
/// (admin-only, server-enforced). The wire protocol: connect → receive
/// `session` → send `subscribe` → receive `logs` batches → send `unsubscribe`.
@MainActor
@Observable
final class DockerLogsViewModel {
    var entries: [DockerLogEntry] = []
    var isConnected = false
    var errorMessage: String?

    private var task: URLSessionWebSocketTask?
    private var receiveLoop: Task<Void, Never>?
    private let maxEntries = 1000

    private let containerId: String
    private let tail: Int
    private let follow: Bool

    init(containerId: String, tail: Int = 200, follow: Bool = true) {
        self.containerId = containerId
        self.tail = tail
        self.follow = follow
    }

    // MARK: - Pure parsing (unit-tested)

    static func parse(_ text: String) -> DockerLogServerMessage {
        guard let data = text.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let type = obj["type"] as? String
        else {
            return .unknown
        }
        switch type {
        case "session":
            return .session
        case "logs":
            guard let raw = obj["entries"] else { return .logs([]) }
            let entriesData = (try? JSONSerialization.data(withJSONObject: raw)) ?? Data()
            let entries = (try? JSONDecoder.snakeCase.decode([DockerLogEntry].self, from: entriesData)) ?? []
            return .logs(entries)
        default:
            return .unknown
        }
    }

    // MARK: - Lifecycle

    func start(serverUrl: String, accessToken: String, serverId: String) {
        guard task == nil else { return }
        guard let url = Self.makeURL(serverUrl: serverUrl, serverId: serverId) else {
            errorMessage = String(localized: "Invalid server URL.")
            return
        }
        var request = URLRequest(url: url)
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        let task = URLSession.shared.webSocketTask(with: request)
        self.task = task
        task.resume()
        receiveLoop = Task { [weak self] in await self?.receiveNext() }
    }

    func stop() {
        sendText("{\"type\":\"unsubscribe\"}")
        receiveLoop?.cancel()
        receiveLoop = nil
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
        isConnected = false
    }

    // MARK: - Receive loop

    private func receiveNext() async {
        guard let task else { return }
        do {
            let message = try await task.receive()
            let text: String?
            switch message {
            case .string(let s): text = s
            case .data(let d): text = String(data: d, encoding: .utf8)
            @unknown default: text = nil
            }
            if let text { handle(Self.parse(text)) }
            if !Task.isCancelled { await receiveNext() }
        } catch {
            if !Task.isCancelled {
                isConnected = false
                errorMessage = String(localized: "Log stream disconnected.")
            }
        }
    }

    private func handle(_ message: DockerLogServerMessage) {
        switch message {
        case .session:
            isConnected = true
            errorMessage = nil
            sendSubscribe()
        case .logs(let batch):
            guard !batch.isEmpty else { return }
            entries.append(contentsOf: batch)
            if entries.count > maxEntries {
                entries.removeFirst(entries.count - maxEntries)
            }
        case .unknown:
            break
        }
    }

    // MARK: - Send

    private func sendSubscribe() {
        let payload = DockerLogSubscribePayload(containerId: containerId, tail: tail, follow: follow)
        guard let data = try? JSONEncoder.snakeCase.encode(payload),
              let text = String(data: data, encoding: .utf8)
        else { return }
        sendText(text)
    }

    private func sendText(_ text: String) {
        task?.send(.string(text)) { _ in }
    }

    static func makeURL(serverUrl: String, serverId: String) -> URL? {
        var ws = serverUrl
        if ws.hasPrefix("https://") {
            ws = "wss://" + ws.dropFirst("https://".count)
        } else if ws.hasPrefix("http://") {
            ws = "ws://" + ws.dropFirst("http://".count)
        }
        if ws.hasSuffix("/") { ws = String(ws.dropLast()) }
        ws += "/api/ws/docker/logs/\(serverId)"
        return URL(string: ws)
    }
}
