import Foundation

/// A configured probe target assigned to a server
/// (`GET /api/servers/{id}/network-probes/targets`).
struct NetworkProbeTarget: Decodable, Identifiable, Sendable {
    let id: String
    let name: String
    let provider: String            // "ct" | "cu" | "cm" | "international" | custom
    let location: String
    let target: String              // FQDN or IP
    let probeType: String           // "icmp" | "tcp" | "http"
    var source: String?             // "preset:{group}" or nil (custom)
    var sourceName: String?

    enum CodingKeys: String, CodingKey {
        case id, name, provider, location, target, source
        case probeType = "probe_type"
        case sourceName = "source_name"
    }
}

/// Latest-value rollup for one target (nested in the server summary).
struct TargetSummary: Decodable, Identifiable, Sendable {
    let targetId: String
    let targetName: String
    let provider: String
    var avgLatency: Double?          // ms
    var minLatency: Double?          // ms
    var maxLatency: Double?          // ms
    let packetLoss: Double           // ratio 0...1
    let availability: Double         // 1 - packetLoss

    var id: String { targetId }

    enum CodingKeys: String, CodingKey {
        case targetId = "target_id"
        case targetName = "target_name"
        case provider
        case avgLatency = "avg_latency"
        case minLatency = "min_latency"
        case maxLatency = "max_latency"
        case packetLoss = "packet_loss"
        case availability
    }
}

/// Per-server probe summary (`GET /api/servers/{id}/network-probes/summary`).
struct NetworkProbeServerSummary: Decodable, Sendable {
    let serverId: String
    let serverName: String
    let online: Bool
    var targets: [TargetSummary]
    var lastProbeAt: String?
    let anomalyCount: Int            // last 24h

    enum CodingKeys: String, CodingKey {
        case serverId = "server_id"
        case serverName = "server_name"
        case online, targets
        case lastProbeAt = "last_probe_at"
        case anomalyCount = "anomaly_count"
    }
}

/// Fleet-wide probe roll-up for one server (`GET /api/network-probes/overview`).
/// Carries per-target latest values plus 24h latency/loss sparklines (newest
/// last; `nil` slots are gaps with no sample).
struct NetworkProbeFleetOverview: Decodable, Identifiable, Sendable {
    let serverId: String
    let serverName: String
    let online: Bool
    var lastProbeAt: String?
    var targets: [TargetSummary]
    let anomalyCount: Int
    var latencySparkline: [Double?]
    var lossSparkline: [Double?]

    var id: String { serverId }

    enum CodingKeys: String, CodingKey {
        case serverId = "server_id"
        case serverName = "server_name"
        case online
        case lastProbeAt = "last_probe_at"
        case targets
        case anomalyCount = "anomaly_count"
        case latencySparkline = "latency_sparkline"
        case lossSparkline = "loss_sparkline"
    }

    /// Worst (highest) average latency across targets, for an at-a-glance value.
    var worstLatency: Double? {
        targets.compactMap(\.avgLatency).max()
    }

    /// Highest packet-loss ratio across targets.
    var worstLoss: Double {
        targets.map(\.packetLoss).max() ?? 0
    }
}

/// One probe sample over time (`.../network-probes/records`). The server picks
/// raw vs hourly aggregates automatically based on the requested window.
struct ProbeRecordDto: Decodable, Sendable {
    let serverId: String
    let targetId: String
    let timestamp: String           // RFC3339
    var avgLatency: Double?          // ms
    var minLatency: Double?          // ms
    var maxLatency: Double?          // ms
    let packetLoss: Double           // ratio 0...1
    let packetSent: Int
    let packetReceived: Int

    enum CodingKeys: String, CodingKey {
        case serverId = "server_id"
        case targetId = "target_id"
        case timestamp
        case avgLatency = "avg_latency"
        case minLatency = "min_latency"
        case maxLatency = "max_latency"
        case packetLoss = "packet_loss"
        case packetSent = "packet_sent"
        case packetReceived = "packet_received"
    }

    var date: Date? { ISO8601DateFormatter.shared.date(from: timestamp) }
}

/// A detected probe anomaly (`.../network-probes/anomalies`).
struct NetworkProbeAnomaly: Decodable, Identifiable, Sendable {
    let timestamp: String           // RFC3339
    let targetId: String
    let targetName: String
    let anomalyType: String         // e.g. "high_latency", "packet_loss"
    let value: Double               // ms or loss ratio depending on type

    var id: String { "\(timestamp)-\(targetId)-\(anomalyType)" }

    enum CodingKeys: String, CodingKey {
        case timestamp
        case targetId = "target_id"
        case targetName = "target_name"
        case anomalyType = "anomaly_type"
        case value
    }

    var date: Date? { ISO8601DateFormatter.shared.date(from: timestamp) }

    var isLatency: Bool { anomalyType.contains("latency") }
}

// MARK: - Provider helpers

enum NetworkProvider {
    /// Canonical bucket for a provider value. The server uses short codes
    /// (`ct`/`cu`/`cm`) in some payloads and full names (`Telecom`/`Unicom`/
    /// `Mobile`) in others, so we normalise both.
    private static func bucket(_ value: String) -> Int {
        switch value.lowercased() {
        case "ct", "telecom", "china telecom": 0
        case "cu", "unicom", "china unicom": 1
        case "cm", "mobile", "china mobile": 2
        case "international": 3
        default: 4
        }
    }

    /// Human label for a probe provider value.
    static func label(for value: String) -> String {
        switch bucket(value) {
        case 0: String(localized: "China Telecom")
        case 1: String(localized: "China Unicom")
        case 2: String(localized: "China Mobile")
        case 3: String(localized: "International")
        default: value.capitalized
        }
    }

    /// Stable ordering for grouping in the UI.
    static func order(for value: String) -> Int { bucket(value) }
}

// MARK: - Latency / loss formatting + colour

enum NetworkFormat {
    static func latency(_ ms: Double?) -> String {
        guard let ms else { return "—" }
        return String(format: "%.1f ms", ms)
    }

    /// `loss` is a 0...1 ratio.
    static func loss(_ ratio: Double) -> String {
        String(format: "%.1f%%", ratio * 100)
    }
}
