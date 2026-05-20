import Foundation

struct ServerStatus: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    /// Optional: WebSocket partial updates may omit this field. Treat `nil` as "unknown — keep previous state".
    var online: Bool?
    var cpuUsage: Double?
    var memoryTotal: Int64?
    var memoryUsed: Int64?
    var diskTotal: Int64?
    var diskUsed: Int64?
    var networkIn: Int64?
    var networkOut: Int64?
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
    var groupName: String?
    var lastActiveAt: String?

    enum CodingKeys: String, CodingKey {
        case id
        case name
        case online
        case cpuUsage = "cpu_usage"
        case cpu
        case memoryTotal = "memory_total"
        case memTotal = "mem_total"
        case memoryUsed = "memory_used"
        case memUsed = "mem_used"
        case diskTotal = "disk_total"
        case diskUsed = "disk_used"
        case networkIn = "network_in"
        case netInSpeed = "net_in_speed"
        case networkOut = "network_out"
        case netOutSpeed = "net_out_speed"
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
        memoryTotal = try container.decodeIfPresent(Int64.self, forKey: .memoryTotal)
            ?? container.decodeIfPresent(Int64.self, forKey: .memTotal)
        memoryUsed = try container.decodeIfPresent(Int64.self, forKey: .memoryUsed)
            ?? container.decodeIfPresent(Int64.self, forKey: .memUsed)
        diskTotal = try container.decodeIfPresent(Int64.self, forKey: .diskTotal)
        diskUsed = try container.decodeIfPresent(Int64.self, forKey: .diskUsed)
        networkIn = try container.decodeIfPresent(Int64.self, forKey: .networkIn)
            ?? container.decodeIfPresent(Int64.self, forKey: .netInSpeed)
        networkOut = try container.decodeIfPresent(Int64.self, forKey: .networkOut)
            ?? container.decodeIfPresent(Int64.self, forKey: .netOutSpeed)
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
        groupName = try container.decodeIfPresent(String.self, forKey: .groupName)
            ?? container.decodeIfPresent(String.self, forKey: .groupId)
        lastActiveAt = try container.decodeIfPresent(String.self, forKey: .lastActiveAt)
            ?? container.decodeIfPresent(String.self, forKey: .lastActive)
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

    /// Disk usage percentage.
    var diskPercent: Double? {
        guard let used = diskUsed, let total = diskTotal, total > 0 else { return nil }
        return Double(used) / Double(total) * 100
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

    /// Merge non-nil fields from another status (used for WebSocket partial updates).
    /// Fields that are `nil` in `other` preserve the local value.
    mutating func merge(from other: ServerStatus) {
        if let v = other.online { online = v }
        if let v = other.cpuUsage { cpuUsage = v }
        if let v = other.memoryTotal, v > 0 { memoryTotal = v }
        if let v = other.memoryUsed { memoryUsed = v }
        if let v = other.diskTotal, v > 0 { diskTotal = v }
        if let v = other.diskUsed { diskUsed = v }
        if let v = other.networkIn { networkIn = v }
        if let v = other.networkOut { networkOut = v }
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
        if let v = other.groupName { groupName = v }
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
