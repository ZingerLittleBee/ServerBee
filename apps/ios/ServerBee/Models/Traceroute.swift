import Foundation

/// Protocol options when triggering a traceroute.
enum TraceProtocol: String, Codable, CaseIterable, Identifiable, Sendable {
    case icmp, udp, tcp
    var id: String { rawValue }
    var label: String { rawValue.uppercased() }
}

/// Request body for `POST /api/servers/{id}/traceroute`.
struct TriggerTracerouteRequest: Encodable, Sendable {
    let target: String
    let protocolValue: TraceProtocol

    enum CodingKeys: String, CodingKey {
        case target
        case protocolValue = "protocol"
    }
}

/// `POST /api/servers/{id}/traceroute` response — the job id to poll.
struct TriggerTracerouteResponse: Decodable, Sendable {
    let requestId: String

    enum CodingKeys: String, CodingKey {
        case requestId = "request_id"
    }
}

/// One traceroute hop. Supports both legacy shell-agent fields (rtt1/2/3, ip)
/// and trippy-core fields (ips, loss_pct, best/avg/worst/jitter). All optional.
struct TracerouteHop: Decodable, Identifiable, Sendable {
    let hop: Int
    var ip: String?
    var hostname: String?
    var asn: String?
    var rtt1: Double?
    var rtt2: Double?
    var rtt3: Double?
    var ips: [String]?
    var totalSent: Int?
    var totalRecv: Int?
    var lossPct: Double?            // 0...100
    var bestMs: Double?
    var worstMs: Double?
    var avgMs: Double?
    var stddevMs: Double?
    var jitterMs: Double?

    var id: Int { hop }

    enum CodingKeys: String, CodingKey {
        case hop, ip, hostname, asn, rtt1, rtt2, rtt3, ips
        case totalSent = "total_sent"
        case totalRecv = "total_recv"
        case lossPct = "loss_pct"
        case bestMs = "best_ms"
        case worstMs = "worst_ms"
        case avgMs = "avg_ms"
        case stddevMs = "stddev_ms"
        case jitterMs = "jitter_ms"
    }

    /// Primary responding IP, preferring the trippy `ips` list.
    var primaryIP: String? { ips?.first ?? ip }

    /// Extra responding IPs beyond the primary (ECMP).
    var extraIPCount: Int {
        guard let ips, ips.count > 1 else { return 0 }
        return ips.count - 1
    }

    /// Best representative latency across legacy/new schemas.
    var displayLatency: Double? {
        if let avgMs { return avgMs }
        let legacy = [rtt1, rtt2, rtt3].compactMap { $0 }
        guard !legacy.isEmpty else { return nil }
        return legacy.reduce(0, +) / Double(legacy.count)
    }

    /// Loss as a 0...1 ratio (trippy reports 0...100).
    var lossRatio: Double? {
        guard let lossPct else { return nil }
        return lossPct / 100
    }

    /// A hop with no response at all (timeout).
    var isUnresponsive: Bool {
        primaryIP == nil && displayLatency == nil
    }
}

/// Live/snapshot traceroute result (`GET .../traceroute/{request_id}`).
struct TracerouteSnapshot: Decodable, Sendable {
    let requestId: String
    let target: String
    let protocolValue: String       // "icmp"|"udp"|"tcp"|"legacy"
    let startedAt: Int64            // unix ms
    var completedAt: Int64?         // unix ms (nil = in progress)
    let round: Int
    let totalRounds: Int
    let completed: Bool
    var hops: [TracerouteHop]
    var error: String?

    enum CodingKeys: String, CodingKey {
        case requestId = "request_id"
        case target
        case protocolValue = "protocol"
        case startedAt = "started_at"
        case completedAt = "completed_at"
        case round
        case totalRounds = "total_rounds"
        case completed, hops, error
    }
}

/// One row in the traceroute history list (`GET .../traceroute`).
struct TracerouteRecordSummary: Decodable, Identifiable, Sendable {
    let requestId: String
    let target: String
    let protocolValue: String
    let startedAt: Int64
    var completedAt: Int64?
    let hopCount: Int
    let hasError: Bool

    var id: String { requestId }

    enum CodingKeys: String, CodingKey {
        case requestId = "request_id"
        case target
        case protocolValue = "protocol"
        case startedAt = "started_at"
        case completedAt = "completed_at"
        case hopCount = "hop_count"
        case hasError = "has_error"
    }

    var startedDate: Date { Date(timeIntervalSince1970: TimeInterval(startedAt) / 1000) }
}
