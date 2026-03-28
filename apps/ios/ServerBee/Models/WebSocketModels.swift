import Foundation

struct ServerStatus: Decodable, Identifiable, Sendable {
    let id: String
    let name: String
    let online: Bool
    let os: String?
    let cpuUsage: Double?
    let memoryTotal: Int64?
    let memoryUsed: Int64?
    let diskTotal: Int64?
    let diskUsed: Int64?
    let networkIn: Int64?
    let networkOut: Int64?
    let uptime: Int64?
    let country: String?
    let region: String?
    let ipv4: String?
    let ipv6: String?
    let cpuName: String?
    let groupName: String?
    let lastActiveAt: String?
    let load1: Double?
    let load5: Double?
    let load15: Double?
    let processCount: Int?
    let tcpCount: Int?
    let udpCount: Int?
}

enum BrowserMessage: Decodable, Sendable {
    case fullSync(servers: [ServerStatus])
    case update(servers: [ServerStatus])
    case serverOnline(serverId: String)
    case serverOffline(serverId: String)
    case capabilitiesChanged(serverId: String, capabilities: Int)
    case agentInfoUpdated(serverId: String, protocolVersion: Int)
    case alertEvent(alertKey: String, status: String)

    enum CodingKeys: String, CodingKey {
        case type
        case servers
        case serverId = "server_id"
        case capabilities
        case protocolVersion = "protocol_version"
        case alertKey = "alert_key"
        case status
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type = try container.decode(String.self, forKey: .type)

        switch type {
        case "full_sync":
            let servers = try container.decode([ServerStatus].self, forKey: .servers)
            self = .fullSync(servers: servers)
        case "update":
            let servers = try container.decode([ServerStatus].self, forKey: .servers)
            self = .update(servers: servers)
        case "server_online":
            let serverId = try container.decode(String.self, forKey: .serverId)
            self = .serverOnline(serverId: serverId)
        case "server_offline":
            let serverId = try container.decode(String.self, forKey: .serverId)
            self = .serverOffline(serverId: serverId)
        case "capabilities_changed":
            let serverId = try container.decode(String.self, forKey: .serverId)
            let capabilities = try container.decode(Int.self, forKey: .capabilities)
            self = .capabilitiesChanged(serverId: serverId, capabilities: capabilities)
        case "agent_info_updated":
            let serverId = try container.decode(String.self, forKey: .serverId)
            let protocolVersion = try container.decode(Int.self, forKey: .protocolVersion)
            self = .agentInfoUpdated(serverId: serverId, protocolVersion: protocolVersion)
        case "alert_event":
            let alertKey = try container.decode(String.self, forKey: .alertKey)
            let status = try container.decode(String.self, forKey: .status)
            self = .alertEvent(alertKey: alertKey, status: status)
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .type,
                in: container,
                debugDescription: "Unknown message type: \(type)"
            )
        }
    }
}
