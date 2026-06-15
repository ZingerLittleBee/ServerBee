import Foundation

/// Per-server capability bitmask, mirrored from `crates/common/src/constants.rs`.
///
/// A server's *configured* capabilities (`capabilities`) are the bits an admin
/// has turned on. The agent reports the bits it can actually serve
/// (`agent_local_capabilities`). The **effective** set is the intersection of
/// the two — defense-in-depth validated on both sides.
///
/// The UI must gate any capability-dependent surface (terminal, files, docker,
/// firewall, ip-quality, security events, …) on the *effective* set so we never
/// offer an action the agent will reject.
enum Capability: Int, CaseIterable, Sendable {
    case terminal = 1
    case exec = 2
    case upgrade = 4
    case pingICMP = 8
    case pingTCP = 16
    case pingHTTP = 32
    case file = 64
    case docker = 128
    case securityEvents = 256
    case firewallBlock = 512
    case ipQuality = 1024

    var label: String {
        switch self {
        case .terminal: String(localized: "Terminal")
        case .exec: String(localized: "Exec")
        case .upgrade: String(localized: "Auto-upgrade")
        case .pingICMP: String(localized: "Ping (ICMP)")
        case .pingTCP: String(localized: "Ping (TCP)")
        case .pingHTTP: String(localized: "Ping (HTTP)")
        case .file: String(localized: "Files")
        case .docker: String(localized: "Docker")
        case .securityEvents: String(localized: "Security events")
        case .firewallBlock: String(localized: "Firewall blocklist")
        case .ipQuality: String(localized: "IP quality")
        }
    }

    var systemImage: String {
        switch self {
        case .terminal: "terminal"
        case .exec: "wand.and.rays"
        case .upgrade: "arrow.up.circle"
        case .pingICMP, .pingTCP, .pingHTTP: "dot.radiowaves.left.and.right"
        case .file: "folder"
        case .docker: "shippingbox"
        case .securityEvents: "shield"
        case .firewallBlock: "lock.shield"
        case .ipQuality: "checkmark.seal"
        }
    }
}

/// A resolved capability set for a single server. Combines the configured mask
/// with the agent-reported and effective masks (any of which may be unknown).
struct CapabilitySet: Equatable, Sendable {
    /// Admin-configured bits (from REST `capabilities` / WS `capabilities_changed`).
    var configured: Int?
    /// Bits the agent reports it can serve. `nil` when the agent has not
    /// reported (older protocol, offline, or never connected).
    var agentLocal: Int?
    /// Server-computed effective bits. `nil` falls back to `configured`.
    var effective: Int?

    /// The mask the UI should gate on: prefer the server-computed effective
    /// set, fall back to the agent intersection, then to configured.
    var resolved: Int {
        if let effective { return effective }
        if let configured, let agentLocal { return configured & agentLocal }
        return configured ?? 0
    }

    /// Whether a capability is effectively available right now.
    func isEnabled(_ cap: Capability) -> Bool {
        resolved & cap.rawValue == cap.rawValue
    }

    /// Whether a capability is *configured* even if not currently effective
    /// (useful to explain "enabled but agent offline / not reporting").
    func isConfigured(_ cap: Capability) -> Bool {
        guard let configured else { return false }
        return configured & cap.rawValue == cap.rawValue
    }

    /// Capabilities that are configured but not effective (e.g. agent offline
    /// or running an older build) — surfaced as a soft warning in the UI.
    var configuredButUnavailable: [Capability] {
        Capability.allCases.filter { isConfigured($0) && !isEnabled($0) }
    }
}
