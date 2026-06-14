import Foundation

// MARK: - Container

/// A Docker container as cached by the server (`GET /api/servers/{id}/docker/containers`).
struct DockerContainer: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let image: String
    let state: String
    let status: String
    let created: Int64
    let ports: [DockerPort]
    let labels: [String: String]

    enum CodingKeys: String, CodingKey {
        case id, name, image, state, status, created, ports, labels
    }

    /// `running`, `exited`, `paused`, `created`, `restarting`, `dead`, …
    var isRunning: Bool { state.lowercased() == "running" }
    var isPaused: Bool { state.lowercased() == "paused" }

    /// Trimmed leading-slash container name Docker reports as "/name".
    var displayName: String {
        name.hasPrefix("/") ? String(name.dropFirst()) : name
    }
}

struct DockerPort: Decodable, Hashable, Sendable {
    let privatePort: Int
    let publicPort: Int?
    let portType: String
    let ip: String?

    enum CodingKeys: String, CodingKey {
        case privatePort = "private_port"
        case publicPort = "public_port"
        case portType = "port_type"
        case ip
    }

    /// e.g. "0.0.0.0:8080→80/tcp" or "80/tcp".
    var display: String {
        if let publicPort {
            let host = ip.map { "\($0):" } ?? ""
            return "\(host)\(publicPort)→\(privatePort)/\(portType)"
        }
        return "\(privatePort)/\(portType)"
    }
}

// MARK: - Stats

struct DockerContainerStats: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let cpuPercent: Double
    let memoryUsage: Int64
    let memoryLimit: Int64
    let memoryPercent: Double
    let networkRx: Int64
    let networkTx: Int64
    let blockRead: Int64
    let blockWrite: Int64

    enum CodingKeys: String, CodingKey {
        case id, name
        case cpuPercent = "cpu_percent"
        case memoryUsage = "memory_usage"
        case memoryLimit = "memory_limit"
        case memoryPercent = "memory_percent"
        case networkRx = "network_rx"
        case networkTx = "network_tx"
        case blockRead = "block_read"
        case blockWrite = "block_write"
    }
}

// MARK: - System info

struct DockerSystemInfo: Decodable, Hashable, Sendable {
    let dockerVersion: String
    let apiVersion: String
    let os: String
    let arch: String
    let containersRunning: Int64
    let containersPaused: Int64
    let containersStopped: Int64
    let images: Int64
    let memoryTotal: Int64

    enum CodingKeys: String, CodingKey {
        case dockerVersion = "docker_version"
        case apiVersion = "api_version"
        case os, arch, images
        case containersRunning = "containers_running"
        case containersPaused = "containers_paused"
        case containersStopped = "containers_stopped"
        case memoryTotal = "memory_total"
    }
}

// MARK: - Events

struct DockerEventInfo: Decodable, Identifiable, Hashable, Sendable {
    let timestamp: Int64
    let eventType: String
    let action: String
    let actorId: String
    let actorName: String?
    let attributes: [String: String]

    enum CodingKeys: String, CodingKey {
        case timestamp, action, attributes
        case eventType = "event_type"
        case actorId = "actor_id"
        case actorName = "actor_name"
    }

    var id: String { "\(timestamp)-\(actorId)-\(action)" }
}

// MARK: - Networks / Volumes

struct DockerNetwork: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let driver: String
    let scope: String
    let containers: [String: String]

    enum CodingKeys: String, CodingKey {
        case id, name, driver, scope, containers
    }
}

struct DockerVolume: Decodable, Identifiable, Hashable, Sendable {
    let name: String
    let driver: String
    let mountpoint: String
    let createdAt: String?
    let labels: [String: String]

    enum CodingKeys: String, CodingKey {
        case name, driver, mountpoint, labels
        case createdAt = "created_at"
    }

    var id: String { name }
}

// MARK: - Logs

/// A single line from the docker logs WebSocket stream.
struct DockerLogEntry: Decodable, Hashable, Sendable {
    let timestamp: String?
    let stream: String
    let message: String

    enum CodingKeys: String, CodingKey {
        case timestamp, stream, message
    }

    /// `stderr` lines are surfaced in a warning colour.
    var isError: Bool { stream.lowercased() == "stderr" }
}

// MARK: - Actions

/// A container action, encoded to match the server's externally-tagged
/// `DockerAction` enum: `"Start"`, `{"Stop":{"timeout":N}}`,
/// `{"Restart":{"timeout":N}}`, `{"Remove":{"force":B}}`.
enum DockerAction: Encodable, Sendable, Identifiable {
    case start
    case stop(timeout: Int?)
    case restart(timeout: Int?)
    case remove(force: Bool)

    var id: String {
        switch self {
        case .start: "start"
        case .stop: "stop"
        case .restart: "restart"
        case .remove: "remove"
        }
    }

    private enum OuterKey: String, CodingKey {
        case stop = "Stop"
        case restart = "Restart"
        case remove = "Remove"
    }

    private struct TimeoutPayload: Encodable { let timeout: Int? }
    private struct ForcePayload: Encodable { let force: Bool }

    func encode(to encoder: Encoder) throws {
        switch self {
        case .start:
            var container = encoder.singleValueContainer()
            try container.encode("Start")
        case .stop(let timeout):
            var container = encoder.container(keyedBy: OuterKey.self)
            try container.encode(TimeoutPayload(timeout: timeout), forKey: .stop)
        case .restart(let timeout):
            var container = encoder.container(keyedBy: OuterKey.self)
            try container.encode(TimeoutPayload(timeout: timeout), forKey: .restart)
        case .remove(let force):
            var container = encoder.container(keyedBy: OuterKey.self)
            try container.encode(ForcePayload(force: force), forKey: .remove)
        }
    }
}

/// Body for `POST /api/servers/{id}/docker/containers/{cid}/action`.
struct ContainerActionRequest: Encodable, Sendable {
    let action: DockerAction
}

// MARK: - Response wrappers

struct DockerContainersResponse: Decodable, Sendable {
    let containers: [DockerContainer]
}

struct DockerStatsResponse: Decodable, Sendable {
    let stats: [DockerContainerStats]
}

struct DockerInfoResponse: Decodable, Sendable {
    let info: DockerSystemInfo
}

struct DockerEventsResponse: Decodable, Sendable {
    let events: [DockerEventInfo]
}

struct DockerNetworksResponse: Decodable, Sendable {
    let networks: [DockerNetwork]
}

struct DockerVolumesResponse: Decodable, Sendable {
    let volumes: [DockerVolume]
}

struct DockerActionResult: Decodable, Sendable {
    let success: Bool
    let error: String?
}
