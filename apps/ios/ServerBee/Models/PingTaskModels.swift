import Foundation

/// Probe type for a ping task. Raw values match the wire exactly (the project
/// decoders apply no key strategy). The target string encodes any port/path:
/// ICMP = bare host/IP, TCP = `host:port`, HTTP = full URL.
enum PingProbeType: String, Codable, Sendable, CaseIterable, Identifiable {
    case icmp
    case tcp
    case http

    var id: String { rawValue }

    var label: String {
        switch self {
        case .icmp: "ICMP"
        case .tcp: "TCP"
        case .http: "HTTP"
        }
    }

    /// Placeholder showing the expected target format for this probe type.
    var targetPlaceholder: String {
        switch self {
        case .icmp: String(localized: "example.com or 1.1.1.1")
        case .tcp: String(localized: "example.com:443")
        case .http: String(localized: "https://example.com/health")
        }
    }
}

/// A ping monitoring task (`GET /api/ping-tasks`). Global, not per-server;
/// `serverIdsJson` is a JSON-string column listing the servers that run it
/// (empty = all servers, filtered by capability at dispatch time).
struct PingTask: Decodable, Identifiable, Sendable {
    let id: String
    let name: String
    let probeType: PingProbeType
    let target: String
    let interval: Int
    let serverIdsJson: String?
    let enabled: Bool
    let createdAt: String

    enum CodingKeys: String, CodingKey {
        case id, name, target, interval, enabled
        case probeType = "probe_type"
        case serverIdsJson = "server_ids_json"
        case createdAt = "created_at"
    }

    /// Servers this task targets (decoded from the JSON-string column); empty
    /// means it applies to all servers.
    var serverIds: [String] {
        guard let raw = serverIdsJson, let data = raw.data(using: .utf8) else { return [] }
        return (try? JSONDecoder().decode([String].self, from: data)) ?? []
    }
}

/// Create body for `POST /api/ping-tasks`. `server_ids` is an ARRAY on the wire
/// (the response carries the `server_ids_json` string instead).
struct CreatePingTaskRequest: Encodable, Sendable {
    let name: String
    let probeType: PingProbeType
    let target: String
    let interval: Int
    let serverIds: [String]
    let enabled: Bool

    enum CodingKeys: String, CodingKey {
        case name, target, interval, enabled
        case probeType = "probe_type"
        case serverIds = "server_ids"
    }
}

/// Partial update body for `PUT /api/ping-tasks/{id}`. Omitted (nil) fields are
/// left unchanged. The enable toggle sends only `enabled`.
struct UpdatePingTaskRequest: Encodable, Sendable {
    var name: String?
    var probeType: PingProbeType?
    var target: String?
    var interval: Int?
    var serverIds: [String]?
    var enabled: Bool?

    enum CodingKeys: String, CodingKey {
        case name, target, interval, enabled
        case probeType = "probe_type"
        case serverIds = "server_ids"
    }
}
