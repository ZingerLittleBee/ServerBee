import Foundation

enum BrowserMessage: Sendable {
    case fullSync(servers: [ServerStatus])
    case update(servers: [ServerStatus])
    case serverOnline(serverId: String)
    case serverOffline(serverId: String)
    case capabilitiesChanged(serverId: String, capabilities: Int)
    case agentInfoUpdated(serverId: String, protocolVersion: Int)
    case alertEvent(alertKey: String, status: AlertStatus)
}

extension BrowserMessage: Decodable {
    private enum MessageType: String, Decodable {
        case fullSync = "full_sync"
        case update
        case serverOnline = "server_online"
        case serverOffline = "server_offline"
        case capabilitiesChanged = "capabilities_changed"
        case agentInfoUpdated = "agent_info_updated"
        case alertEvent = "alert_event"
    }

    private enum CodingKeys: String, CodingKey {
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
        let type = try container.decode(MessageType.self, forKey: .type)

        switch type {
        case .fullSync:
            let servers = try container.decode([ServerStatus].self, forKey: .servers)
            self = .fullSync(servers: servers)
        case .update:
            let servers = try container.decode([ServerStatus].self, forKey: .servers)
            self = .update(servers: servers)
        case .serverOnline:
            let serverId = try container.decode(String.self, forKey: .serverId)
            self = .serverOnline(serverId: serverId)
        case .serverOffline:
            let serverId = try container.decode(String.self, forKey: .serverId)
            self = .serverOffline(serverId: serverId)
        case .capabilitiesChanged:
            let serverId = try container.decode(String.self, forKey: .serverId)
            let capabilities = try container.decode(Int.self, forKey: .capabilities)
            self = .capabilitiesChanged(serverId: serverId, capabilities: capabilities)
        case .agentInfoUpdated:
            let serverId = try container.decode(String.self, forKey: .serverId)
            let protocolVersion = try container.decode(Int.self, forKey: .protocolVersion)
            self = .agentInfoUpdated(serverId: serverId, protocolVersion: protocolVersion)
        case .alertEvent:
            let alertKey = try container.decode(String.self, forKey: .alertKey)
            let status = try container.decode(AlertStatus.self, forKey: .status)
            self = .alertEvent(alertKey: alertKey, status: status)
        }
    }
}

struct ApiResponse<T: Decodable & Sendable>: Decodable, Sendable {
    let data: T
}
