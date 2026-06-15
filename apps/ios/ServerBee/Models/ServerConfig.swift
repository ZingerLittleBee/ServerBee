import Foundation

/// Server group (`/api/server-groups`). Used to resolve `group_id` to a name.
struct ServerGroup: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    var weight: Int?
    var createdAt: String?

    enum CodingKeys: String, CodingKey {
        case id, name, weight
        case createdAt = "created_at"
    }
}

/// Create body for `POST /api/server-groups` (weight is forced to 0 server-side).
struct CreateGroupRequest: Encodable, Sendable {
    let name: String
}

/// Partial update body for `PUT /api/server-groups/{id}` (omit to leave unchanged).
struct UpdateGroupRequest: Encodable, Sendable {
    var name: String?
    var weight: Int?
}

/// A pending enrollment code summary attached to a server that has not yet been
/// claimed by an agent. The plaintext code is NEVER returned here — only on the
/// create/recover/regenerate calls.
struct OutstandingEnrollment: Decodable, Hashable, Sendable {
    let id: String
    let codePrefix: String?
    let expiresAt: String?
    let createdAt: String?

    enum CodingKeys: String, CodingKey {
        case id
        case codePrefix = "code_prefix"
        case expiresAt = "expires_at"
        case createdAt = "created_at"
    }
}

/// Full server *configuration* as returned by REST `/api/servers/{id}`.
///
/// Distinct from `ServerStatus` (the live/merged display model): this is the
/// authoritative source for static metadata — capabilities, billing, kernel,
/// agent version, enrollment state — that the WebSocket never sends.
struct ServerConfig: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    var cpuName: String?
    var cpuCores: Int?
    var cpuArch: String?
    var os: String?
    var kernelVersion: String?
    var memTotal: Int64?
    var swapTotal: Int64?
    var diskTotal: Int64?
    var ipv4: String?
    var ipv6: String?
    var region: String?
    var countryCode: String?
    var virtualization: String?
    var agentVersion: String?
    var groupId: String?
    var weight: Int?
    var hidden: Bool?
    var remark: String?
    var publicRemark: String?
    var price: Double?
    var billingCycle: String?
    var currency: String?
    var expiredAt: String?
    var trafficLimit: Int64?
    var trafficLimitType: String?
    var billingStartDay: Int?
    var capabilities: Int?
    var agentLocalCapabilities: Int?
    var effectiveCapabilities: Int?
    var protocolVersion: Int?
    var features: [String]?
    var hasToken: Bool?
    var outstandingEnrollment: OutstandingEnrollment?
    var createdAt: String?
    var updatedAt: String?

    enum CodingKeys: String, CodingKey {
        case id, name, region, currency, weight, hidden, remark, price, features, capabilities, virtualization
        case cpuName = "cpu_name"
        case cpuCores = "cpu_cores"
        case cpuArch = "cpu_arch"
        case os
        case kernelVersion = "kernel_version"
        case memTotal = "mem_total"
        case swapTotal = "swap_total"
        case diskTotal = "disk_total"
        case ipv4, ipv6
        case countryCode = "country_code"
        case agentVersion = "agent_version"
        case groupId = "group_id"
        case publicRemark = "public_remark"
        case billingCycle = "billing_cycle"
        case expiredAt = "expired_at"
        case trafficLimit = "traffic_limit"
        case trafficLimitType = "traffic_limit_type"
        case billingStartDay = "billing_start_day"
        case agentLocalCapabilities = "agent_local_capabilities"
        case effectiveCapabilities = "effective_capabilities"
        case protocolVersion = "protocol_version"
        case hasToken = "has_token"
        case outstandingEnrollment = "outstanding_enrollment"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    var capabilitySet: CapabilitySet {
        CapabilitySet(
            configured: capabilities,
            agentLocal: agentLocalCapabilities,
            effective: effectiveCapabilities
        )
    }

    /// `false` => pending enrollment (agent never connected).
    var isEnrolled: Bool { hasToken ?? true }

    var expiredDate: Date? {
        guard let expiredAt else { return nil }
        return ISO8601DateFormatter.shared.date(from: expiredAt)
    }
}
