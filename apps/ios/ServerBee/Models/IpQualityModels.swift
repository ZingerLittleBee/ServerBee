import Foundation
import SwiftUI

/// IP reputation/quality snapshot for a server's egress IP
/// (nested in `GET /api/ip-quality/servers/{id}`).
struct IpQualitySnapshot: Decodable, Sendable {
    let ip: String
    var asn: String?
    var asOrg: String?
    var country: String?
    var region: String?
    var city: String?
    let ipType: String              // "datacenter" | "residential" | "business" | …
    let isProxy: Bool
    let isVpn: Bool
    let isHosting: Bool
    let isTor: Bool
    let isAbuser: Bool
    let isMobile: Bool
    var riskScore: Int?             // 0...100
    let riskLevel: String           // "low" | "medium" | "high"
    var asnAbuserScore: Int?
    var abuseEmail: String?
    var checkedAt: String?

    enum CodingKeys: String, CodingKey {
        case ip, asn
        case asOrg = "as_org"
        case country, region, city
        case ipType = "ip_type"
        case isProxy = "is_proxy"
        case isVpn = "is_vpn"
        case isHosting = "is_hosting"
        case isTor = "is_tor"
        case isAbuser = "is_abuser"
        case isMobile = "is_mobile"
        case riskScore = "risk_score"
        case riskLevel = "risk_level"
        case asnAbuserScore = "asn_abuser_score"
        case abuseEmail = "abuse_email"
        case checkedAt = "checked_at"
    }

    /// Active risk flags, for chip rendering.
    var flags: [String] {
        var f: [String] = []
        if isProxy { f.append(String(localized: "Proxy")) }
        if isVpn { f.append(String(localized: "VPN")) }
        if isTor { f.append(String(localized: "Tor")) }
        if isHosting { f.append(String(localized: "Hosting")) }
        if isAbuser { f.append(String(localized: "Abuser")) }
        if isMobile { f.append(String(localized: "Mobile")) }
        return f
    }

    var location: String? {
        let parts = [city, region, country].compactMap { $0 }.filter { !$0.isEmpty }
        return parts.isEmpty ? nil : parts.joined(separator: ", ")
    }
}

/// Result of one streaming/unlock service check.
struct UnlockResultDto: Decodable, Identifiable, Sendable {
    let id: String
    let serverId: String
    let serviceId: String
    let status: String              // unlocked|restricted|blocked|failed|unsupported
    var region: String?
    var latencyMs: Int?
    var detail: String?
    let checkedAt: String

    enum CodingKeys: String, CodingKey {
        case id
        case serverId = "server_id"
        case serviceId = "service_id"
        case status, region
        case latencyMs = "latency_ms"
        case detail
        case checkedAt = "checked_at"
    }
}

/// Per-server IP quality payload (`GET /api/ip-quality/servers/{id}`).
struct ServerIpQualityData: Decodable, Sendable {
    let serverId: String
    var unlockResults: [UnlockResultDto]
    var ipQuality: IpQualitySnapshot?

    enum CodingKeys: String, CodingKey {
        case serverId = "server_id"
        case unlockResults = "unlock_results"
        case ipQuality = "ip_quality"
    }
}

/// One catalog entry (`GET /api/ip-quality/services`), used both to resolve a
/// `service_id` to a human name and to drive the admin service catalog.
struct UnlockService: Decodable, Identifiable, Sendable {
    let id: String
    let key: String
    let name: String
    var category: String?
    let enabled: Bool
    var popularity: Int?
    var isBuiltin: Bool?

    enum CodingKeys: String, CodingKey {
        case id, key, name, category, enabled, popularity
        case isBuiltin = "is_builtin"
    }

    /// Built-in services ship with the server: their definition can't be edited
    /// or deleted (only their enabled flag is toggleable). Custom services can
    /// be deleted.
    var builtin: Bool { isBuiltin ?? false }

    /// Category label, capitalized; "Other" when unset.
    var categoryLabel: String {
        guard let category, !category.isEmpty else { return String(localized: "Other") }
        return category.capitalized
    }
}

// MARK: - Presentation helpers

enum IpRisk {
    static func color(_ level: String) -> Color {
        switch level {
        case "low": .serverOnline
        case "medium": .warningAmber
        case "high": .serverOffline
        default: .secondary
        }
    }

    static func label(_ level: String) -> String { level.capitalized }
}

enum UnlockStatusStyle {
    static func color(_ status: String) -> Color {
        switch status {
        case "unlocked": .serverOnline
        case "restricted": .warningAmber
        case "blocked", "failed": .serverOffline
        default: .secondary
        }
    }

    static func label(_ status: String) -> String {
        switch status {
        case "unlocked": String(localized: "Unlocked")
        case "restricted": String(localized: "Restricted")
        case "blocked": String(localized: "Blocked")
        case "failed": String(localized: "Failed")
        case "unsupported": String(localized: "Unsupported")
        default: status.capitalized
        }
    }
}
