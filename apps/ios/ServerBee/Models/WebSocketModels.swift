import Foundation

// MARK: - BrowserMessage

/// Messages sent from the server to browser/mobile WebSocket clients.
/// Decoded manually via the `"type"` discriminator field.
enum BrowserMessage: Sendable {
    case fullSync(servers: [ServerStatus])
    case update(servers: [PartialServerStatus])
    case serverOnline(serverId: String)
    case serverOffline(serverId: String)
    case capabilitiesChanged(serverId: String, capabilities: UInt32)
    case agentInfoUpdated(serverId: String, protocolVersion: UInt32)
    case alertEvent(alertKey: String, status: AlertStatus)
    case unknown
}

extension BrowserMessage: Decodable {
    private enum CodingKeys: String, CodingKey {
        case type
        case servers
        case serverId
        case capabilities
        case protocolVersion
        case alertKey
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
            let servers = try container.decode([PartialServerStatus].self, forKey: .servers)
            self = .update(servers: servers)
        case "server_online":
            let serverId = try container.decode(String.self, forKey: .serverId)
            self = .serverOnline(serverId: serverId)
        case "server_offline":
            let serverId = try container.decode(String.self, forKey: .serverId)
            self = .serverOffline(serverId: serverId)
        case "capabilities_changed":
            let serverId = try container.decode(String.self, forKey: .serverId)
            let capabilities = try container.decode(UInt32.self, forKey: .capabilities)
            self = .capabilitiesChanged(serverId: serverId, capabilities: capabilities)
        case "agent_info_updated":
            let serverId = try container.decode(String.self, forKey: .serverId)
            let protocolVersion = try container.decode(UInt32.self, forKey: .protocolVersion)
            self = .agentInfoUpdated(serverId: serverId, protocolVersion: protocolVersion)
        case "alert_event":
            let alertKey = try container.decode(String.self, forKey: .alertKey)
            let status = try container.decode(AlertStatus.self, forKey: .status)
            self = .alertEvent(alertKey: alertKey, status: status)
        default:
            self = .unknown
        }
    }
}

// MARK: - ApiResponse

/// Generic API response wrapper matching `{ data: T }` from the server.
struct ApiResponse<T: Decodable>: Decodable {
    let data: T
}

// MARK: - JSONDecoder convenience

extension JSONDecoder {
    /// A decoder configured for the ServerBee snake_case JSON convention.
    static let snakeCase: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }()
}
