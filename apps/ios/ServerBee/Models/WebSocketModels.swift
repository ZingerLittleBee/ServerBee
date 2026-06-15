import Foundation

enum BrowserMessage: Sendable {
    case fullSync(servers: [ServerStatus], upgrades: [UpgradeJob])
    case update(servers: [ServerStatus])
    case serverOnline(serverId: String)
    case serverOffline(serverId: String)
    case capabilitiesChanged(serverId: String, capabilities: Int, agentLocal: Int?, effective: Int?)
    case agentInfoUpdated(serverId: String, protocolVersion: Int)
    case alertEvent(alertKey: String, status: AlertStatus)
    case securityEvent(SecurityEventBroadcast)
    /// Live agent self-upgrade progress. Carries no status — it always implies
    /// `running` and only merges onto an existing job.
    case upgradeProgress(serverId: String, jobId: String, targetVersion: String, stage: UpgradeStage)
    /// Terminal agent self-upgrade result (succeeded/failed/timeout).
    case upgradeResult(
        serverId: String,
        jobId: String,
        targetVersion: String,
        status: UpgradeStatus,
        stage: UpgradeStage?,
        error: String?,
        backupPath: String?
    )
    /// Any server message type this client doesn't consume yet (docker, ip
    /// quality, blocklist, …). Decoded so the receive loop doesn't log a
    /// spurious error for every such frame.
    case unknown
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
        case securityEvent = "security_event"
        case upgradeProgress = "upgrade_progress"
        case upgradeResult = "upgrade_result"
    }

    private enum CodingKeys: String, CodingKey {
        case type
        case servers
        case upgrades
        case serverId = "server_id"
        case capabilities
        case agentLocalCapabilities = "agent_local_capabilities"
        case effectiveCapabilities = "effective_capabilities"
        case protocolVersion = "protocol_version"
        case alertKey = "alert_key"
        case status
        case jobId = "job_id"
        case targetVersion = "target_version"
        case stage
        case error
        case backupPath = "backup_path"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        // Unknown / not-yet-handled message types decode to `.unknown` instead
        // of throwing, so the WS receive loop stays quiet.
        guard let type = try? container.decode(MessageType.self, forKey: .type) else {
            self = .unknown
            return
        }

        switch type {
        case .fullSync:
            let servers = try container.decode([ServerStatus].self, forKey: .servers)
            // `upgrades` is `#[serde(default)]` server-side — old servers omit it.
            let upgrades = (try? container.decode([UpgradeJob].self, forKey: .upgrades)) ?? []
            self = .fullSync(servers: servers, upgrades: upgrades)
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
            let agentLocal = try container.decodeIfPresent(Int.self, forKey: .agentLocalCapabilities)
            let effective = try container.decodeIfPresent(Int.self, forKey: .effectiveCapabilities)
            self = .capabilitiesChanged(
                serverId: serverId,
                capabilities: capabilities,
                agentLocal: agentLocal,
                effective: effective
            )
        case .agentInfoUpdated:
            let serverId = try container.decode(String.self, forKey: .serverId)
            let protocolVersion = try container.decode(Int.self, forKey: .protocolVersion)
            self = .agentInfoUpdated(serverId: serverId, protocolVersion: protocolVersion)
        case .alertEvent:
            let alertKey = try container.decode(String.self, forKey: .alertKey)
            let status = try container.decode(AlertStatus.self, forKey: .status)
            self = .alertEvent(alertKey: alertKey, status: status)
        case .securityEvent:
            self = .securityEvent(try SecurityEventBroadcast(from: decoder))
        case .upgradeProgress:
            // Progress frames carry no status — they always imply `running`.
            self = .upgradeProgress(
                serverId: try container.decode(String.self, forKey: .serverId),
                jobId: try container.decode(String.self, forKey: .jobId),
                targetVersion: try container.decode(String.self, forKey: .targetVersion),
                stage: try container.decode(UpgradeStage.self, forKey: .stage)
            )
        case .upgradeResult:
            self = .upgradeResult(
                serverId: try container.decode(String.self, forKey: .serverId),
                jobId: try container.decode(String.self, forKey: .jobId),
                targetVersion: try container.decode(String.self, forKey: .targetVersion),
                status: try container.decode(UpgradeStatus.self, forKey: .status),
                stage: try container.decodeIfPresent(UpgradeStage.self, forKey: .stage),
                error: try container.decodeIfPresent(String.self, forKey: .error),
                backupPath: try container.decodeIfPresent(String.self, forKey: .backupPath)
            )
        }
    }
}
