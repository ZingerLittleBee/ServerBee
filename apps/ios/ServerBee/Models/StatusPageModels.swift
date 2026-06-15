import Foundation

/// Public status-page configuration (`GET /api/status-page`, singleton).
/// One row per deployment; `serverIdsJson` is a JSON-string column listing the
/// servers exposed on the public page (empty = none pinned).
struct StatusPageConfig: Decodable, Sendable {
    let id: String
    var title: String
    var description: String?
    let serverIdsJson: String
    var enabled: Bool
    var uptimeYellowThreshold: Double
    var uptimeRedThreshold: Double
    var showIpQuality: Bool
    var defaultLayout: String        // "list" | "grid"
    var showServerDetail: Bool
    var showNetwork: Bool
    var showIncidents: Bool
    var showMaintenance: Bool

    enum CodingKeys: String, CodingKey {
        case id, title, description, enabled
        case serverIdsJson = "server_ids_json"
        case uptimeYellowThreshold = "uptime_yellow_threshold"
        case uptimeRedThreshold = "uptime_red_threshold"
        case showIpQuality = "show_ip_quality"
        case defaultLayout = "default_layout"
        case showServerDetail = "show_server_detail"
        case showNetwork = "show_network"
        case showIncidents = "show_incidents"
        case showMaintenance = "show_maintenance"
    }

    /// Servers exposed on the page (decoded from the JSON-string column).
    var serverIds: [String] {
        guard let data = serverIdsJson.data(using: .utf8) else { return [] }
        return (try? JSONDecoder().decode([String].self, from: data)) ?? []
    }
}

/// Layout mode for the public status page.
enum StatusPageLayout: String, CaseIterable, Identifiable, Sendable {
    case list
    case grid

    var id: String { rawValue }

    var label: String {
        switch self {
        case .list: String(localized: "List")
        case .grid: String(localized: "Grid")
        }
    }
}

/// Body for `PUT /api/status-page`. The edit form loads current state and
/// submits the full set, so every field is sent. `description` is sent as JSON
/// null when blank (the server models it as a nullable column). `server_ids` is
/// an ARRAY on the wire (the response carries `server_ids_json` instead).
struct UpdateStatusPageRequest: Encodable, Sendable {
    var title: String
    var description: String?
    var serverIds: [String]
    var enabled: Bool
    var uptimeYellowThreshold: Double
    var uptimeRedThreshold: Double
    var showIpQuality: Bool
    var defaultLayout: String
    var showServerDetail: Bool
    var showNetwork: Bool
    var showIncidents: Bool
    var showMaintenance: Bool

    enum CodingKeys: String, CodingKey {
        case title, description, enabled
        case serverIds = "server_ids"
        case uptimeYellowThreshold = "uptime_yellow_threshold"
        case uptimeRedThreshold = "uptime_red_threshold"
        case showIpQuality = "show_ip_quality"
        case defaultLayout = "default_layout"
        case showServerDetail = "show_server_detail"
        case showNetwork = "show_network"
        case showIncidents = "show_incidents"
        case showMaintenance = "show_maintenance"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(title, forKey: .title)
        // Send explicit null to clear when blank; the column is nullable.
        let trimmed = description?.trimmingCharacters(in: .whitespacesAndNewlines)
        if let trimmed, !trimmed.isEmpty {
            try container.encode(trimmed, forKey: .description)
        } else {
            try container.encodeNil(forKey: .description)
        }
        try container.encode(serverIds, forKey: .serverIds)
        try container.encode(enabled, forKey: .enabled)
        try container.encode(uptimeYellowThreshold, forKey: .uptimeYellowThreshold)
        try container.encode(uptimeRedThreshold, forKey: .uptimeRedThreshold)
        try container.encode(showIpQuality, forKey: .showIpQuality)
        try container.encode(defaultLayout, forKey: .defaultLayout)
        try container.encode(showServerDetail, forKey: .showServerDetail)
        try container.encode(showNetwork, forKey: .showNetwork)
        try container.encode(showIncidents, forKey: .showIncidents)
        try container.encode(showMaintenance, forKey: .showMaintenance)
    }
}
