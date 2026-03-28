import Foundation

struct ServerStatus: Codable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let online: Bool
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

struct MetricRecord: Codable, Identifiable, Sendable {
    var id: String { timestamp }
    let timestamp: String
    var cpuUsage: Double?
    var memoryUsed: Int64?
    var networkIn: Int64?
    var networkOut: Int64?
    var diskUsed: Int64?

    enum CodingKeys: String, CodingKey {
        case timestamp
        case cpuUsage = "cpu_usage"
        case memoryUsed = "memory_used"
        case networkIn = "network_in"
        case networkOut = "network_out"
        case diskUsed = "disk_used"
    }
}
