import SwiftUI

// MARK: - Incident

/// An operational incident (`GET /api/incidents`).
struct Incident: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let title: String
    let status: String
    let severity: String
    let serverIdsJson: String?
    let isPublic: Bool
    let createdAt: String
    let updatedAt: String
    let resolvedAt: String?

    enum CodingKeys: String, CodingKey {
        case id, title, status, severity
        case serverIdsJson = "server_ids_json"
        case isPublic = "is_public"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case resolvedAt = "resolved_at"
    }

    var isResolved: Bool { status.lowercased() == "resolved" }

    var severityColor: Color {
        switch severity.lowercased() {
        case "critical": .serverOffline
        case "major": .warningAmber
        default: .brandAccent
        }
    }

    var statusLabel: String {
        switch status.lowercased() {
        case "investigating": String(localized: "Investigating")
        case "identified": String(localized: "Identified")
        case "monitoring": String(localized: "Monitoring")
        case "resolved": String(localized: "Resolved")
        default: status.capitalized
        }
    }
}

/// A status update appended to an incident.
struct IncidentUpdate: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let incidentId: String
    let status: String
    let message: String
    let createdAt: String

    enum CodingKeys: String, CodingKey {
        case id, status, message
        case incidentId = "incident_id"
        case createdAt = "created_at"
    }
}

/// Valid incident statuses (matches the server enum).
enum IncidentStatus: String, CaseIterable, Identifiable, Sendable {
    case investigating, identified, monitoring, resolved
    var id: String { rawValue }
    var label: String {
        switch self {
        case .investigating: String(localized: "Investigating")
        case .identified: String(localized: "Identified")
        case .monitoring: String(localized: "Monitoring")
        case .resolved: String(localized: "Resolved")
        }
    }
}

enum IncidentSeverity: String, CaseIterable, Identifiable, Sendable {
    case minor, major, critical
    var id: String { rawValue }
    var label: String {
        switch self {
        case .minor: String(localized: "Minor")
        case .major: String(localized: "Major")
        case .critical: String(localized: "Critical")
        }
    }
}

// MARK: - Requests

struct CreateIncidentRequest: Encodable, Sendable {
    let title: String
    var severity: String?
    var isPublic: Bool?

    enum CodingKeys: String, CodingKey {
        case title, severity
        case isPublic = "is_public"
    }
}

struct CreateIncidentUpdateRequest: Encodable, Sendable {
    let status: String
    let message: String
}

// MARK: - Maintenance

/// A scheduled maintenance window (`GET /api/maintenances`).
struct Maintenance: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let title: String
    let description: String?
    let startAt: String
    let endAt: String
    let serverIdsJson: String?
    let isPublic: Bool
    let active: Bool
    let createdAt: String
    let updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, title, description, active
        case startAt = "start_at"
        case endAt = "end_at"
        case serverIdsJson = "server_ids_json"
        case isPublic = "is_public"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}
