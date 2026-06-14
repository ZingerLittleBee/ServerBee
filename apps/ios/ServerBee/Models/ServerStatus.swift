import Foundation

/// Unified server display model.
///
/// The same type is decoded from two very different sources:
/// - **REST `/api/servers`** — static *configuration* (name, IPs, capabilities,
///   billing, agent version, kernel, …) but NO live metrics and NO `online`.
/// - **WebSocket `/api/ws/servers`** — live *runtime* state (`online`, `cpu`,
///   `mem_used`, load, transfer, disk I/O, `last_active`, …) but NONE of the
///   config fields (no `ipv4`, no `capabilities`, no billing).
///
/// Because neither source is complete, the view-model MERGES them by id rather
/// than replacing wholesale — see `ServersViewModel`. The decoder below is
/// deliberately lenient: every field is optional and accepts the alias keys used
/// by each source. `merge(from:)` only copies non-nil values so a partial WS
/// frame never erases a config field (e.g. `ipv4`) the REST fetch supplied.
struct ServerStatus: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    /// Optional: WebSocket partial updates may omit this field. Treat `nil` as "unknown — keep previous state".
    var online: Bool?
    var cpuUsage: Double?
    var cpuCores: Int?
    var memoryTotal: Int64?
    var memoryUsed: Int64?
    var swapTotal: Int64?
    var swapUsed: Int64?
    var diskTotal: Int64?
    var diskUsed: Int64?
    var diskReadPerSec: Int64?
    var diskWritePerSec: Int64?
    var networkIn: Int64?
    var networkOut: Int64?
    var netInTransfer: Int64?
    var netOutTransfer: Int64?
    var load1: Double?
    var load5: Double?
    var load15: Double?
    var processCount: Int?
    var tcpCount: Int?
    var udpCount: Int?
    var uptime: Int64?
    var os: String?
    var cpuName: String?
    var ipv4: String?
    var ipv6: String?
    var region: String?
    var country: String?
    /// Group identifier (UUID). Resolve to a human name via the groups map in
    /// `ServersViewModel`; never display the raw id.
    var groupId: String?
    /// Legacy alias retained for the memberwise initializer used by tests; the
    /// REST/WS payloads populate `groupId`.
    var groupName: String?
    var tags: [String]?
    /// Configured capability bits (REST `capabilities` / WS `capabilities_changed`).
    var capabilities: Int?
    var agentLocalCapabilities: Int?
    var effectiveCapabilities: Int?
    /// `false` => pending enrollment (agent never connected).
    var hasToken: Bool?
    /// ISO-8601 string. The WS frame sends `last_active` as a Unix **epoch
    /// second integer**; the decoder normalises both forms to a string here so
    /// view code keeps a single representation.
    var lastActiveAt: String?

    enum CodingKeys: String, CodingKey {
        case id
        case name
        case online
        case cpuUsage = "cpu_usage"
        case cpu
        case cpuCores = "cpu_cores"
        case memoryTotal = "memory_total"
        case memTotal = "mem_total"
        case memoryUsed = "memory_used"
        case memUsed = "mem_used"
        case swapTotal = "swap_total"
        case swapUsed = "swap_used"
        case diskTotal = "disk_total"
        case diskUsed = "disk_used"
        case diskReadPerSec = "disk_read_bytes_per_sec"
        case diskWritePerSec = "disk_write_bytes_per_sec"
        case networkIn = "network_in"
        case netInSpeed = "net_in_speed"
        case networkOut = "network_out"
        case netOutSpeed = "net_out_speed"
        case netInTransfer = "net_in_transfer"
        case netOutTransfer = "net_out_transfer"
        case load1
        case load5
        case load15
        case processCount = "process_count"
        case tcpConn = "tcp_conn"
        case tcpCount = "tcp_count"
        case udpConn = "udp_conn"
        case udpCount = "udp_count"
        case uptime
        case os
        case cpuName = "cpu_name"
        case ipv4
        case ipv6
        case region
        case countryCode = "country_code"
        case country
        case groupName = "group_name"
        case groupId = "group_id"
        case tags
        case capabilities
        case agentLocalCapabilities = "agent_local_capabilities"
        case effectiveCapabilities = "effective_capabilities"
        case hasToken = "has_token"
        case lastActiveAt = "last_active_at"
        case lastActive = "last_active"
    }

    /// Convenience: treat unknown (`nil`) as offline for view code.
    var isOnline: Bool { online ?? false }
}

extension ServerStatus {
    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)

        id = try container.decode(String.self, forKey: .id)
        name = try container.decode(String.self, forKey: .name)
        online = try container.decodeIfPresent(Bool.self, forKey: .online)
        cpuUsage = try container.decodeIfPresent(Double.self, forKey: .cpuUsage)
            ?? container.decodeIfPresent(Double.self, forKey: .cpu)
        cpuCores = try container.decodeIfPresent(Int.self, forKey: .cpuCores)
        memoryTotal = try container.decodeIfPresent(Int64.self, forKey: .memoryTotal)
            ?? container.decodeIfPresent(Int64.self, forKey: .memTotal)
        memoryUsed = try container.decodeIfPresent(Int64.self, forKey: .memoryUsed)
            ?? container.decodeIfPresent(Int64.self, forKey: .memUsed)
        swapTotal = try container.decodeIfPresent(Int64.self, forKey: .swapTotal)
        swapUsed = try container.decodeIfPresent(Int64.self, forKey: .swapUsed)
        diskTotal = try container.decodeIfPresent(Int64.self, forKey: .diskTotal)
        diskUsed = try container.decodeIfPresent(Int64.self, forKey: .diskUsed)
        diskReadPerSec = try container.decodeIfPresent(Int64.self, forKey: .diskReadPerSec)
        diskWritePerSec = try container.decodeIfPresent(Int64.self, forKey: .diskWritePerSec)
        networkIn = try container.decodeIfPresent(Int64.self, forKey: .networkIn)
            ?? container.decodeIfPresent(Int64.self, forKey: .netInSpeed)
        networkOut = try container.decodeIfPresent(Int64.self, forKey: .networkOut)
            ?? container.decodeIfPresent(Int64.self, forKey: .netOutSpeed)
        netInTransfer = try container.decodeIfPresent(Int64.self, forKey: .netInTransfer)
        netOutTransfer = try container.decodeIfPresent(Int64.self, forKey: .netOutTransfer)
        load1 = try container.decodeIfPresent(Double.self, forKey: .load1)
        load5 = try container.decodeIfPresent(Double.self, forKey: .load5)
        load15 = try container.decodeIfPresent(Double.self, forKey: .load15)
        processCount = try container.decodeIfPresent(Int.self, forKey: .processCount)
        tcpCount = try container.decodeIfPresent(Int.self, forKey: .tcpCount)
            ?? container.decodeIfPresent(Int.self, forKey: .tcpConn)
        udpCount = try container.decodeIfPresent(Int.self, forKey: .udpCount)
            ?? container.decodeIfPresent(Int.self, forKey: .udpConn)
        uptime = try container.decodeIfPresent(Int64.self, forKey: .uptime)
        os = try container.decodeIfPresent(String.self, forKey: .os)
        cpuName = try container.decodeIfPresent(String.self, forKey: .cpuName)
        ipv4 = try container.decodeIfPresent(String.self, forKey: .ipv4)
        ipv6 = try container.decodeIfPresent(String.self, forKey: .ipv6)
        region = try container.decodeIfPresent(String.self, forKey: .region)
        country = try container.decodeIfPresent(String.self, forKey: .country)
            ?? container.decodeIfPresent(String.self, forKey: .countryCode)
        groupId = try container.decodeIfPresent(String.self, forKey: .groupId)
        groupName = try container.decodeIfPresent(String.self, forKey: .groupName)
        tags = try container.decodeIfPresent([String].self, forKey: .tags)
        capabilities = try container.decodeIfPresent(Int.self, forKey: .capabilities)
        agentLocalCapabilities = try container.decodeIfPresent(Int.self, forKey: .agentLocalCapabilities)
        effectiveCapabilities = try container.decodeIfPresent(Int.self, forKey: .effectiveCapabilities)
        hasToken = try container.decodeIfPresent(Bool.self, forKey: .hasToken)

        // `last_active` is a Unix epoch integer over the WS, an ISO string over
        // REST (`last_active_at`). Normalise to an ISO string. Using `try?` so a
        // type surprise degrades to nil instead of failing the whole frame.
        if let s = try? container.decodeIfPresent(String.self, forKey: .lastActiveAt) {
            lastActiveAt = s
        } else if let s = try? container.decodeIfPresent(String.self, forKey: .lastActive) {
            lastActiveAt = s
        } else if let epoch = try? container.decodeIfPresent(Int64.self, forKey: .lastActive) {
            lastActiveAt = ISO8601DateFormatter.shared.string(from: Date(timeIntervalSince1970: TimeInterval(epoch)))
        } else {
            lastActiveAt = nil
        }
    }

    /// First non-nil IP address (prefer IPv4).
    var primaryIP: String? {
        ipv4 ?? ipv6
    }

    /// Memory usage percentage.
    var memoryPercent: Double? {
        guard let used = memoryUsed, let total = memoryTotal, total > 0 else { return nil }
        return Double(used) / Double(total) * 100
    }

    /// Swap usage percentage.
    var swapPercent: Double? {
        guard let used = swapUsed, let total = swapTotal, total > 0 else { return nil }
        return Double(used) / Double(total) * 100
    }

    /// Disk usage percentage.
    var diskPercent: Double? {
        guard let used = diskUsed, let total = diskTotal, total > 0 else { return nil }
        return Double(used) / Double(total) * 100
    }

    /// Resolved capability set for gating capability-dependent UI.
    var capabilitySet: CapabilitySet {
        CapabilitySet(
            configured: capabilities,
            agentLocal: agentLocalCapabilities,
            effective: effectiveCapabilities
        )
    }

    /// Human-readable location derived from region + country.
    var location: String? {
        switch (region, country) {
        case let (r?, c?): return "\(r), \(c)"
        case let (r?, nil): return r
        case let (nil, c?): return c
        default: return nil
        }
    }

    /// Parsed `Date` of the last heartbeat, if known.
    var lastActiveDate: Date? {
        guard let lastActiveAt else { return nil }
        return ISO8601DateFormatter.shared.date(from: lastActiveAt)
    }

    /// Merge non-nil fields from another status (used for WebSocket partial
    /// updates AND for overlaying a live WS frame onto REST config). Fields that
    /// are `nil` in `other` preserve the local value, so a metrics-only frame
    /// never erases config fields like `ipv4`, `capabilities`, or billing.
    mutating func merge(from other: ServerStatus) {
        if let v = other.online { online = v }
        if let v = other.cpuUsage { cpuUsage = v }
        if let v = other.cpuCores { cpuCores = v }
        if let v = other.memoryTotal, v > 0 { memoryTotal = v }
        if let v = other.memoryUsed { memoryUsed = v }
        if let v = other.swapTotal { swapTotal = v }
        if let v = other.swapUsed { swapUsed = v }
        if let v = other.diskTotal, v > 0 { diskTotal = v }
        if let v = other.diskUsed { diskUsed = v }
        if let v = other.diskReadPerSec { diskReadPerSec = v }
        if let v = other.diskWritePerSec { diskWritePerSec = v }
        if let v = other.networkIn { networkIn = v }
        if let v = other.networkOut { networkOut = v }
        if let v = other.netInTransfer { netInTransfer = v }
        if let v = other.netOutTransfer { netOutTransfer = v }
        if let v = other.load1 { load1 = v }
        if let v = other.load5 { load5 = v }
        if let v = other.load15 { load15 = v }
        if let v = other.processCount { processCount = v }
        if let v = other.tcpCount { tcpCount = v }
        if let v = other.udpCount { udpCount = v }
        if let v = other.uptime { uptime = v }
        if let v = other.os { os = v }
        if let v = other.cpuName { cpuName = v }
        if let v = other.ipv4 { ipv4 = v }
        if let v = other.ipv6 { ipv6 = v }
        if let v = other.region { region = v }
        if let v = other.country { country = v }
        if let v = other.groupId { groupId = v }
        if let v = other.groupName { groupName = v }
        if let v = other.tags { tags = v }
        if let v = other.capabilities { capabilities = v }
        if let v = other.agentLocalCapabilities { agentLocalCapabilities = v }
        if let v = other.effectiveCapabilities { effectiveCapabilities = v }
        if let v = other.hasToken { hasToken = v }
        if let v = other.lastActiveAt { lastActiveAt = v }
    }
}

struct MetricRecord: Decodable, Identifiable, Sendable {
    var id: String { timestamp }
    let timestamp: String
    var cpuUsage: Double?
    var memoryUsed: Int64?
    var memoryTotal: Int64?
    var networkIn: Int64?
    var networkOut: Int64?
    var diskUsed: Int64?
    var diskTotal: Int64?
    var load1: Double?
    var diskReadPerSec: Int64?
    var diskWritePerSec: Int64?

    enum CodingKeys: String, CodingKey {
        case timestamp
        case time
        case cpuUsage = "cpu_usage"
        case cpu
        case memoryUsed = "memory_used"
        case memUsed = "mem_used"
        case memoryTotal = "memory_total"
        case memTotal = "mem_total"
        case networkIn = "network_in"
        case netInSpeed = "net_in_speed"
        case networkOut = "network_out"
        case netOutSpeed = "net_out_speed"
        case diskUsed = "disk_used"
        case diskTotal = "disk_total"
        case load1
        case diskReadPerSec = "disk_read_bytes_per_sec"
        case diskWritePerSec = "disk_write_bytes_per_sec"
    }
}

extension MetricRecord {
    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)

        timestamp = try container.decodeIfPresent(String.self, forKey: .timestamp)
            ?? container.decode(String.self, forKey: .time)
        cpuUsage = try container.decodeIfPresent(Double.self, forKey: .cpuUsage)
            ?? container.decodeIfPresent(Double.self, forKey: .cpu)
        memoryUsed = try container.decodeIfPresent(Int64.self, forKey: .memoryUsed)
            ?? container.decodeIfPresent(Int64.self, forKey: .memUsed)
        memoryTotal = try container.decodeIfPresent(Int64.self, forKey: .memoryTotal)
            ?? container.decodeIfPresent(Int64.self, forKey: .memTotal)
        networkIn = try container.decodeIfPresent(Int64.self, forKey: .networkIn)
            ?? container.decodeIfPresent(Int64.self, forKey: .netInSpeed)
        networkOut = try container.decodeIfPresent(Int64.self, forKey: .networkOut)
            ?? container.decodeIfPresent(Int64.self, forKey: .netOutSpeed)
        diskUsed = try container.decodeIfPresent(Int64.self, forKey: .diskUsed)
        diskTotal = try container.decodeIfPresent(Int64.self, forKey: .diskTotal)
        load1 = try container.decodeIfPresent(Double.self, forKey: .load1)
        diskReadPerSec = try container.decodeIfPresent(Int64.self, forKey: .diskReadPerSec)
        diskWritePerSec = try container.decodeIfPresent(Int64.self, forKey: .diskWritePerSec)
    }

    /// Parsed Date from the ISO 8601 timestamp string.
    var date: Date? {
        ISO8601DateFormatter.shared.date(from: timestamp)
    }

    /// Memory usage percentage (0-100).
    var memoryPercent: Double? {
        guard let used = memoryUsed, let total = memoryTotal, total > 0 else { return nil }
        return Double(used) / Double(total) * 100
    }

    /// Disk usage percentage (0-100).
    var diskPercent: Double? {
        guard let used = diskUsed, let total = diskTotal, total > 0 else { return nil }
        return Double(used) / Double(total) * 100
    }
}
