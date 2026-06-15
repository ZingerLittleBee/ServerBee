import Foundation
import SwiftUI

/// One firewall blocklist entry (`GET /api/firewall/blocks`).
struct BlockListItem: Decodable, Identifiable, Sendable {
    let id: String
    let target: String              // canonical CIDR, e.g. "1.2.3.4/32"
    let family: Int                 // 4 or 6
    let coverType: String           // "all" | "include" | "exclude"
    var serverIds: [String]?
    var comment: String?
    let origin: String              // "manual" | "auto"
    var originEventId: String?
    var originRuleId: String?
    var createdBy: String?
    let createdAt: String

    enum CodingKeys: String, CodingKey {
        case id, target, family, comment, origin
        case coverType = "cover_type"
        case serverIds = "server_ids"
        case originEventId = "origin_event_id"
        case originRuleId = "origin_rule_id"
        case createdBy = "created_by"
        case createdAt = "created_at"
    }

    var isAuto: Bool { origin == "auto" }

    var coverLabel: String {
        switch coverType {
        case "all": String(localized: "All servers")
        case "include": String(localized: "\(serverIds?.count ?? 0) servers")
        case "exclude": String(localized: "All except \(serverIds?.count ?? 0)")
        default: coverType
        }
    }
}

/// Cursor-paginated block list.
struct BlockListResponse: Decodable, Sendable {
    let items: [BlockListItem]
    var nextCursor: String?

    enum CodingKeys: String, CodingKey {
        case items
        case nextCursor = "next_cursor"
    }
}

/// Aggregate counts (`GET /api/firewall/stats`).
struct FirewallStats: Decodable, Sendable {
    let total: Int
    let auto: Int
    let manual: Int
    let v4: Int
    let v6: Int
}

/// Request body for `POST /api/firewall/blocks`.
struct CreateBlockRequest: Encodable, Sendable {
    let target: String
    let coverType: String
    var serverIds: [String]?
    var comment: String?

    enum CodingKeys: String, CodingKey {
        case target, comment
        case coverType = "cover_type"
        case serverIds = "server_ids"
    }
}

/// Cover-type options for a new block.
enum BlockCoverType: String, CaseIterable, Identifiable, Sendable {
    case all
    case include
    case exclude

    var id: String { rawValue }

    var label: String {
        switch self {
        case .all: String(localized: "All servers")
        case .include: String(localized: "Only selected")
        case .exclude: String(localized: "All except selected")
        }
    }
}
