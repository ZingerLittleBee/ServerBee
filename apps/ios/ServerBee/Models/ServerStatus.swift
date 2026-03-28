import Foundation

/// Real-time status snapshot for a monitored server.
struct ServerStatus: Codable, Identifiable, Sendable {
    let id: String
    let name: String
    let online: Bool
    let lastActive: Int64
    let uptime: UInt64
    let cpu: Double
    let memUsed: Int64
    let memTotal: Int64
    let swapUsed: Int64
    let swapTotal: Int64
    let diskUsed: Int64
    let diskTotal: Int64
    let netInSpeed: Int64
    let netOutSpeed: Int64
    let netInTransfer: Int64
    let netOutTransfer: Int64
    let load1: Double
    let load5: Double
    let load15: Double
    let tcpConn: Int32
    let udpConn: Int32
    let processCount: Int32
    let cpuName: String?
    let os: String?
    let region: String?
    let countryCode: String?
    let groupId: String?
    let features: [String]
}

/// Partial server status used in Update messages.
/// All fields are optional since the server only sends changed values.
struct PartialServerStatus: Codable, Identifiable, Sendable {
    let id: String
    let name: String?
    let online: Bool?
    let lastActive: Int64?
    let uptime: UInt64?
    let cpu: Double?
    let memUsed: Int64?
    let memTotal: Int64?
    let swapUsed: Int64?
    let swapTotal: Int64?
    let diskUsed: Int64?
    let diskTotal: Int64?
    let netInSpeed: Int64?
    let netOutSpeed: Int64?
    let netInTransfer: Int64?
    let netOutTransfer: Int64?
    let load1: Double?
    let load5: Double?
    let load15: Double?
    let tcpConn: Int32?
    let udpConn: Int32?
    let processCount: Int32?
    let cpuName: String?
    let os: String?
    let region: String?
    let countryCode: String?
    let groupId: String?
    let features: [String]?
}
