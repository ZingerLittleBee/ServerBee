import Foundation

struct ServerStatus: Codable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    var online: Bool
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
        case memoryTotal = "memory_total"
        case memoryUsed = "memory_used"
        case diskTotal = "disk_total"
        case diskUsed = "disk_used"
        case networkIn = "network_in"
        case networkOut = "network_out"
        case load1
        case load5
        case load15
        case processCount = "process_count"
        case tcpCount = "tcp_count"
        case udpCount = "udp_count"
        case uptime
        case os
        case cpuName = "cpu_name"
        case ipv4
        case ipv6
        case region
        case country
        case groupName = "group_name"
        case lastActiveAt = "last_active_at"
    }
}

extension ServerStatus {
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
    mutating func merge(from other: ServerStatus) {
        online = other.online
        if let v = other.cpuUsage { cpuUsage = v }
        if let v = other.memoryTotal { memoryTotal = v }
        if let v = other.memoryUsed { memoryUsed = v }
        if let v = other.diskTotal { diskTotal = v }
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

struct MetricRecord: Codable, Identifiable, Sendable {
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
        case cpuUsage = "cpu_usage"
        case memoryUsed = "memory_used"
        case memoryTotal = "memory_total"
        case networkIn = "network_in"
        case networkOut = "network_out"
        case diskUsed = "disk_used"
        case diskTotal = "disk_total"
    }
}

extension MetricRecord {
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
