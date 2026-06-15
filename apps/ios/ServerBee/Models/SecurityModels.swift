import Foundation
import SwiftUI

// MARK: - Evidence

/// Flattened, lenient view of a security event's polymorphic `evidence` blob.
/// The server tags it with `kind` and carries different fields per kind; every
/// field here is optional so any current or future kind decodes cleanly.
struct SecurityEvidence: Decodable, Sendable {
    var kind: String?
    // ssh_login
    var authMethod: String?
    // ssh_brute_force
    var failedCount: Int?
    var distinctUsers: Int?
    var sampleUsers: [String]?
    var invalidUserCount: Int?
    // port_scan
    var distinctPorts: Int?
    var samplePorts: [Int]?
    var totalAttempts: Int?
    var blockedCount: Int?
    // shared
    var windowSeconds: Int?
    var threshold: Int?

    enum CodingKeys: String, CodingKey {
        case kind
        case authMethod = "auth_method"
        case failedCount = "failed_count"
        case distinctUsers = "distinct_users"
        case sampleUsers = "sample_users"
        case invalidUserCount = "invalid_user_count"
        case distinctPorts = "distinct_ports"
        case samplePorts = "sample_ports"
        case totalAttempts = "total_attempts"
        case blockedCount = "blocked_count"
        case windowSeconds = "window_seconds"
        case threshold
    }

    /// Ordered (label, value) pairs for the detail view, skipping empty fields.
    var detailRows: [(String, String)] {
        var rows: [(String, String)] = []
        if let authMethod { rows.append((String(localized: "Auth method"), authMethod)) }
        if let failedCount { rows.append((String(localized: "Failed attempts"), "\(failedCount)")) }
        if let distinctUsers { rows.append((String(localized: "Distinct users"), "\(distinctUsers)")) }
        if let invalidUserCount { rows.append((String(localized: "Invalid users"), "\(invalidUserCount)")) }
        if let users = sampleUsers, !users.isEmpty { rows.append((String(localized: "Sample users"), users.joined(separator: ", "))) }
        if let distinctPorts { rows.append((String(localized: "Distinct ports"), "\(distinctPorts)")) }
        if let ports = samplePorts, !ports.isEmpty {
            rows.append((String(localized: "Sample ports"), ports.map(String.init).joined(separator: ", ")))
        }
        if let totalAttempts { rows.append((String(localized: "Total attempts"), "\(totalAttempts)")) }
        if let blockedCount { rows.append((String(localized: "Blocked"), "\(blockedCount)")) }
        if let windowSeconds { rows.append((String(localized: "Window"), "\(windowSeconds)s")) }
        if let threshold { rows.append((String(localized: "Threshold"), "\(threshold)")) }
        return rows
    }

    /// One-line summary for the feed row.
    var summary: String? {
        if let failedCount { return String(localized: "\(failedCount) failed logins") }
        if let distinctPorts { return String(localized: "\(distinctPorts) ports scanned") }
        if let authMethod { return String(localized: "via \(authMethod)") }
        return nil
    }
}

// MARK: - REST DTOs

/// A persisted security event (`GET /api/security/events`).
struct SecurityEventDto: Decodable, Sendable {
    let id: String
    let serverId: String
    let eventType: String
    let severity: String
    let sourceIp: String
    var sourcePort: Int?
    var username: String?
    let startedAt: String
    let endedAt: String
    let firstSeen: Bool
    let detectorSource: String
    var evidence: SecurityEvidence?
    let createdAt: String

    enum CodingKeys: String, CodingKey {
        case id
        case serverId = "server_id"
        case eventType = "event_type"
        case severity
        case sourceIp = "source_ip"
        case sourcePort = "source_port"
        case username
        case startedAt = "started_at"
        case endedAt = "ended_at"
        case firstSeen = "first_seen"
        case detectorSource = "detector_source"
        case evidence
        case createdAt = "created_at"
    }
}

/// Cursor-paginated event page.
struct SecurityEventList: Decodable, Sendable {
    let items: [SecurityEventDto]
    var nextCursor: String?

    enum CodingKeys: String, CodingKey {
        case items
        case nextCursor = "next_cursor"
    }
}

/// One aggregation bucket (`GET /api/security/stats`).
struct StatsBucket: Decodable, Identifiable, Sendable {
    let key: String
    let count: Int

    var id: String { key }
}

// MARK: - WebSocket broadcast

/// Live `security_event` push from `/api/ws/servers`.
struct SecurityEventBroadcast: Decodable, Sendable {
    let serverId: String
    let eventId: String
    let event: SecurityEventPayload

    enum CodingKeys: String, CodingKey {
        case serverId = "server_id"
        case eventId = "event_id"
        case event
    }
}

/// The agent-reported payload nested in a broadcast. Enum-typed fields are
/// decoded as raw strings for forward-compatibility; timestamps are unix secs.
struct SecurityEventPayload: Decodable, Sendable {
    let eventType: String
    let severity: String
    let sourceIp: String
    var sourcePort: Int?
    var username: String?
    let startedAt: Int64
    let endedAt: Int64
    let firstSeen: Bool
    let detectorSource: String
    var evidence: SecurityEvidence?

    enum CodingKeys: String, CodingKey {
        case eventType = "event_type"
        case severity
        case sourceIp = "source_ip"
        case sourcePort = "source_port"
        case username
        case startedAt = "started_at"
        case endedAt = "ended_at"
        case firstSeen = "first_seen"
        case detectorSource = "detector_source"
        case evidence
    }
}

// MARK: - Unified display model

/// Display-ready security event, built from either a REST DTO or a live WS push.
struct SecurityEvent: Identifiable, Sendable {
    let id: String
    let serverId: String
    let eventType: String
    let severity: String
    let sourceIp: String
    let sourcePort: Int?
    let username: String?
    let date: Date?
    let firstSeen: Bool
    let detectorSource: String
    let evidence: SecurityEvidence?

    init(dto: SecurityEventDto) {
        id = dto.id
        serverId = dto.serverId
        eventType = dto.eventType
        severity = dto.severity
        sourceIp = dto.sourceIp
        sourcePort = dto.sourcePort
        username = dto.username
        date = ISO8601DateFormatter.shared.date(from: dto.createdAt)
        firstSeen = dto.firstSeen
        detectorSource = dto.detectorSource
        evidence = dto.evidence
    }

    init(broadcast: SecurityEventBroadcast) {
        let e = broadcast.event
        id = broadcast.eventId
        serverId = broadcast.serverId
        eventType = e.eventType
        severity = e.severity
        sourceIp = e.sourceIp
        sourcePort = e.sourcePort
        username = e.username
        date = Date(timeIntervalSince1970: TimeInterval(e.startedAt))
        firstSeen = e.firstSeen
        detectorSource = e.detectorSource
        evidence = e.evidence
    }
}

// MARK: - Presentation helpers

enum SecuritySeverity {
    static func color(_ severity: String) -> Color {
        switch severity {
        case "critical": .red
        case "high": .serverOffline
        case "medium": .warningAmber
        case "low": .secondary
        default: .secondary
        }
    }

    /// Higher = more severe, for sorting/emphasis.
    static func rank(_ severity: String) -> Int {
        switch severity {
        case "critical": 4
        case "high": 3
        case "medium": 2
        case "low": 1
        default: 0
        }
    }

    static func label(_ severity: String) -> String { severity.capitalized }
}

enum SecurityEventKind {
    static func label(_ type: String) -> String {
        switch type {
        case "ssh_login": String(localized: "SSH Login")
        case "ssh_brute_force": String(localized: "SSH Brute Force")
        case "port_scan": String(localized: "Port Scan")
        default: type.replacingOccurrences(of: "_", with: " ").capitalized
        }
    }

    static func icon(_ type: String) -> String {
        switch type {
        case "ssh_login": "person.badge.key"
        case "ssh_brute_force": "lock.trianglebadge.exclamationmark"
        case "port_scan": "dot.radiowaves.left.and.right"
        default: "shield.lefthalf.filled"
        }
    }

    static func color(_ type: String) -> Color {
        switch type {
        case "ssh_brute_force": .red
        case "port_scan": .orange
        case "ssh_login": .blue
        default: .secondary
        }
    }
}

enum DetectorLabel {
    static func label(_ source: String) -> String {
        switch source {
        case "journal": "journald"
        case "auth_log": "auth.log"
        case "conntrack": "conntrack"
        case "firewall_log": String(localized: "Firewall log")
        default: source
        }
    }
}
