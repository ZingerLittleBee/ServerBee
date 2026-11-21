import Foundation

// MARK: - Per-server Tailscale status (`GET /api/servers/{id}/tailscale/status`)

struct ServerTailscaleStatusResponse: Codable, Equatable, Sendable {
    let available: Bool
    let online: Bool
    let cached: Bool
    var tailscaleIpv4: String?
    var tailscaleIpv6: String?
    var tailscaleDnsName: String?
    var tailscaleTailnet: String?
    var status: TailscaleStatusReport?

    enum CodingKeys: String, CodingKey {
        case available
        case online
        case cached
        case tailscaleIpv4 = "tailscale_ipv4"
        case tailscaleIpv6 = "tailscale_ipv6"
        case tailscaleDnsName = "tailscale_dns_name"
        case tailscaleTailnet = "tailscale_tailnet"
        case status
    }
}

struct TailscaleStatusReport: Codable, Equatable, Sendable {
    let summary: TailscaleNodeSummary
    let advertisedRoutes: [String]
    let tags: [String]
    var derpRegion: String?
    let peers: [TailscalePeerInfo]
    let healthWarnings: [String]
    let collectedAt: String

    enum CodingKeys: String, CodingKey {
        case summary
        case advertisedRoutes = "advertised_routes"
        case tags
        case derpRegion = "derp_region"
        case peers
        case healthWarnings = "health_warnings"
        case collectedAt = "collected_at"
    }
}

struct TailscaleNodeSummary: Codable, Equatable, Sendable {
    let backendState: String
    let tailscaleIps: [String]
    var dnsName: String?
    var tailnetName: String?
    var version: String?
    var hostname: String?
    let online: Bool
    let exitNode: Bool
    let exitNodeOption: Bool
    var keyExpiry: String?

    enum CodingKeys: String, CodingKey {
        case backendState = "backend_state"
        case tailscaleIps = "tailscale_ips"
        case dnsName = "dns_name"
        case tailnetName = "tailnet_name"
        case version
        case hostname
        case online
        case exitNode = "exit_node"
        case exitNodeOption = "exit_node_option"
        case keyExpiry = "key_expiry"
    }

    var backendStateLabel: String {
        Self.backendStateLabel(for: backendState)
    }

    static func backendStateLabel(for state: String) -> String {
        switch state {
        case "Running": String(localized: "Running")
        case "Stopped": String(localized: "Stopped")
        case "NeedsLogin": String(localized: "Needs login")
        case "NeedsMachineAuth": String(localized: "Needs authorization")
        case "Starting": String(localized: "Starting")
        case "NoState": String(localized: "Not initialized")
        case "InUseOtherUser": String(localized: "In use by another user")
        default: state
        }
    }

    var cleanDnsName: String? {
        TailscaleFormatting.cleanDNSName(dnsName)
    }

    var exitNodeStatusLabel: String {
        if exitNode { return String(localized: "Active") }
        if exitNodeOption { return String(localized: "Advertised") }
        return String(localized: "Off")
    }
}

struct TailscalePeerInfo: Codable, Identifiable, Equatable, Sendable {
    let id: String
    let hostName: String
    var dnsName: String?
    var os: String?
    let tailscaleIps: [String]
    let online: Bool
    let active: Bool
    let connectionMode: String
    var curAddr: String?
    var relay: String?
    let rxBytes: UInt64
    let txBytes: UInt64
    var lastHandshake: String?
    var lastSeen: String?
    let exitNode: Bool
    let exitNodeOption: Bool
    let tags: [String]

    enum CodingKeys: String, CodingKey {
        case id
        case hostName = "host_name"
        case dnsName = "dns_name"
        case os
        case tailscaleIps = "tailscale_ips"
        case online
        case active
        case connectionMode = "connection_mode"
        case curAddr = "cur_addr"
        case relay
        case rxBytes = "rx_bytes"
        case txBytes = "tx_bytes"
        case lastHandshake = "last_handshake"
        case lastSeen = "last_seen"
        case exitNode = "exit_node"
        case exitNodeOption = "exit_node_option"
        case tags
    }

    var connectionModeLabel: String {
        Self.connectionModeLabel(mode: connectionMode, relay: relay)
    }

    static func connectionModeLabel(mode: String, relay: String? = nil) -> String {
        switch mode.lowercased() {
        case "direct":
            return String(localized: "Direct")
        case "derp":
            let trimmed = relay?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            if !trimmed.isEmpty {
                return String(localized: "DERP · \(trimmed)")
            }
            return String(localized: "DERP")
        case "peer_relay":
            return String(localized: "Peer Relay")
        case "idle":
            return String(localized: "Idle")
        default:
            return mode.capitalized
        }
    }

    var primaryIp: String? {
        tailscaleIps.first { !$0.contains(":") } ?? tailscaleIps.first
    }

    var cleanDnsName: String? {
        TailscaleFormatting.cleanDNSName(dnsName)
    }

    /// Filters out Go's zero timestamp (`0001-01-01...`) emitted by `tailscaled`
    /// before a WireGuard session has completed its first handshake.
    var validLastHandshake: String? {
        TailscaleFormatting.validTimestamp(lastHandshake)
    }

    var validLastSeen: String? {
        TailscaleFormatting.validTimestamp(lastSeen)
    }
}

// MARK: - Global Tailnet overview (`GET /api/tailscale/overview`)

struct TailnetOverviewResponse: Codable, Equatable, Sendable {
    let configured: Bool
    let provider: String
    let tailnet: String
    var serverTailnetUrl: String?
    let totalDevices: Int
    let onlineDevices: Int
    let linkedServers: Int
    let expiringKeysCount: Int
    var fetchedAt: String?
    var error: String?
    let devices: [TailnetDeviceItem]

    enum CodingKeys: String, CodingKey {
        case configured
        case provider
        case tailnet
        case serverTailnetUrl = "server_tailnet_url"
        case totalDevices = "total_devices"
        case onlineDevices = "online_devices"
        case linkedServers = "linked_servers"
        case expiringKeysCount = "expiring_keys_count"
        case fetchedAt = "fetched_at"
        case error
        case devices
    }
}

struct TailnetDeviceItem: Codable, Identifiable, Equatable, Sendable {
    let id: String
    let name: String
    let hostname: String
    var os: String?
    let addresses: [String]
    let authorized: Bool
    let online: Bool
    var lastSeen: String?
    let keyExpiryDisabled: Bool
    var expires: String?
    var clientVersion: String?
    let tags: [String]
    var matchedServerId: String?
    var matchedServerName: String?

    enum CodingKeys: String, CodingKey {
        case id
        case name
        case hostname
        case os
        case addresses
        case authorized
        case online
        case lastSeen = "last_seen"
        case keyExpiryDisabled = "key_expiry_disabled"
        case expires
        case clientVersion = "client_version"
        case tags
        case matchedServerId = "matched_server_id"
        case matchedServerName = "matched_server_name"
    }

    var primaryAddress: String? {
        addresses.first { !$0.contains(":") } ?? addresses.first
    }

    var cleanName: String {
        TailscaleFormatting.cleanDNSName(name) ?? name
    }

    var validLastSeen: String? {
        TailscaleFormatting.validTimestamp(lastSeen)
    }

    var validExpires: String? {
        TailscaleFormatting.validTimestamp(expires)
    }
}

// MARK: - Admin configuration (`GET`/`PUT /api/tailscale/config` & `POST /api/tailscale/test`)

struct TailscaleAdminConfigResponse: Codable, Equatable, Sendable {
    let enabled: Bool
    let provider: String
    let apiBaseUrl: String
    let tailnet: String
    let hasApiKey: Bool
    let preferredServerUrl: String
    let authEnabled: Bool
    let autoProvisionRole: String

    enum CodingKeys: String, CodingKey {
        case enabled
        case provider
        case apiBaseUrl = "api_base_url"
        case tailnet
        case hasApiKey = "has_api_key"
        case preferredServerUrl = "preferred_server_url"
        case authEnabled = "auth_enabled"
        case autoProvisionRole = "auto_provision_role"
    }

    init(
        enabled: Bool,
        provider: String,
        apiBaseUrl: String,
        tailnet: String,
        hasApiKey: Bool,
        preferredServerUrl: String,
        authEnabled: Bool,
        autoProvisionRole: String
    ) {
        self.enabled = enabled
        self.provider = provider
        self.apiBaseUrl = apiBaseUrl
        self.tailnet = tailnet
        self.hasApiKey = hasApiKey
        self.preferredServerUrl = preferredServerUrl
        self.authEnabled = authEnabled
        self.autoProvisionRole = autoProvisionRole
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        enabled = try container.decode(Bool.self, forKey: .enabled)
        provider = try container.decode(String.self, forKey: .provider)
        apiBaseUrl = try container.decodeIfPresent(String.self, forKey: .apiBaseUrl) ?? ""
        tailnet = try container.decodeIfPresent(String.self, forKey: .tailnet) ?? ""
        hasApiKey = try container.decodeIfPresent(Bool.self, forKey: .hasApiKey) ?? false
        preferredServerUrl = try container.decodeIfPresent(String.self, forKey: .preferredServerUrl) ?? ""
        authEnabled = try container.decodeIfPresent(Bool.self, forKey: .authEnabled) ?? false
        autoProvisionRole = try container.decodeIfPresent(String.self, forKey: .autoProvisionRole) ?? ""
    }
}

struct UpdateTailscaleConfigRequest: Codable, Equatable, Sendable {
    var enabled: Bool
    var provider: String
    var apiBaseUrl: String
    var tailnet: String
    var apiKey: String?
    var preferredServerUrl: String?
    var authEnabled: Bool?
    var autoProvisionRole: String?

    enum CodingKeys: String, CodingKey {
        case enabled
        case provider
        case apiBaseUrl = "api_base_url"
        case tailnet
        case apiKey = "api_key"
        case preferredServerUrl = "preferred_server_url"
        case authEnabled = "auth_enabled"
        case autoProvisionRole = "auto_provision_role"
    }
}

struct TestTailscaleConfigRequest: Codable, Equatable, Sendable {
    var provider: String?
    var apiBaseUrl: String?
    var tailnet: String?
    var apiKey: String?

    enum CodingKeys: String, CodingKey {
        case provider
        case apiBaseUrl = "api_base_url"
        case tailnet
        case apiKey = "api_key"
    }
}

struct TailscaleConnectionTestResponse: Codable, Equatable, Sendable {
    let success: Bool
    let deviceCount: Int
    let message: String

    enum CodingKeys: String, CodingKey {
        case success
        case deviceCount = "device_count"
        case message
    }
}

// MARK: - UI enums & helpers

enum TailscaleProvider: String, CaseIterable, Identifiable, Sendable {
    case tailscale
    case headscale

    var id: String { rawValue }

    var label: String {
        switch self {
        case .tailscale: "Tailscale"
        case .headscale: "Headscale"
        }
    }

    var defaultApiBaseUrl: String {
        switch self {
        case .tailscale: "https://api.tailscale.com"
        case .headscale: ""
        }
    }

    static func displayName(for rawValue: String) -> String {
        Self(rawValue: rawValue.lowercased())?.label ?? rawValue.capitalized
    }
}

enum TailscaleAutoProvisionRole: String, CaseIterable, Identifiable, Sendable {
    case disabled = ""
    case member = "member"

    var id: String { rawValue }

    var label: String {
        switch self {
        case .disabled: String(localized: "Disabled")
        case .member: String(localized: "Member")
        }
    }
}

enum TailscaleFormatting {
    static func cleanDNSName(_ raw: String?) -> String? {
        guard let raw else { return nil }
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            .trimmingCharacters(in: CharacterSet(charactersIn: "."))
        return trimmed.isEmpty ? nil : trimmed
    }

    static func validTimestamp(_ raw: String?) -> String? {
        guard let raw else { return nil }
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, !trimmed.hasPrefix("0001-01-01") else { return nil }
        return trimmed
    }
}

enum TailscaleVisibility {
    /// Determines whether Tailscale UI surfaces should appear for a server.
    /// Requires `CAP_TAILSCALE` (effective while online; configured or effective
    /// while offline so cached status stays viewable) and either the `"tailscale"`
    /// runtime feature or a persisted `tailscale_ipv4`.
    static func shouldShow(
        capabilities: CapabilitySet,
        sectionCapabilities: CapabilitySet? = nil,
        isOnline: Bool = true,
        features: [String]?,
        tailscaleIpv4: String?
    ) -> Bool {
        let capAllowed: Bool
        if isOnline {
            capAllowed = capabilities.isEnabled(.tailscale)
        } else {
            let configCaps = sectionCapabilities ?? capabilities
            capAllowed = configCaps.isConfigured(.tailscale) || capabilities.isEnabled(.tailscale)
        }
        guard capAllowed else { return false }
        let hasFeature = features?.contains("tailscale") == true
        let hasIpv4 = !(tailscaleIpv4?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true)
        return hasFeature || hasIpv4
    }
}
