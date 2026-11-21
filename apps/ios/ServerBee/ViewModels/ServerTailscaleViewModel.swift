import Foundation
import Observation

/// Drives `ServerTailscaleView` (`GET /api/servers/{id}/tailscale/status`).
@MainActor
@Observable
final class ServerTailscaleViewModel {
    private(set) var response: ServerTailscaleStatusResponse?
    var isLoading = false
    var hasLoaded = false
    var errorMessage: String?

    var report: TailscaleStatusReport? {
        response?.status
    }

    var summary: TailscaleNodeSummary? {
        response?.status?.summary
    }

    /// Primary Tailscale IPv4 from top-level response or `summary.tailscaleIps`.
    var primaryIpv4: String? {
        if let v4 = response?.tailscaleIpv4, !v4.isEmpty { return v4 }
        return summary?.tailscaleIps.first { !$0.contains(":") }
    }

    /// Primary Tailscale IPv6 from top-level response or `summary.tailscaleIps`.
    var primaryIpv6: String? {
        if let v6 = response?.tailscaleIpv6, !v6.isEmpty { return v6 }
        return summary?.tailscaleIps.first { $0.contains(":") }
    }

    /// Normalized MagicDNS name without trailing dot.
    var dnsName: String? {
        TailscaleFormatting.cleanDNSName(summary?.dnsName ?? response?.tailscaleDnsName)
    }

    /// Tailnet name from live report or persisted server metadata.
    var tailnetName: String? {
        let candidate = summary?.tailnetName ?? response?.tailscaleTailnet
        guard let candidate else { return nil }
        let trimmed = candidate.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    /// Peers sorted online-first, active-first, then alphabetically by hostname.
    var sortedPeers: [TailscalePeerInfo] {
        guard let peers = report?.peers else { return [] }
        return peers.sorted { lhs, rhs in
            if lhs.online != rhs.online { return lhs.online && !rhs.online }
            if lhs.active != rhs.active { return lhs.active && !rhs.active }
            return lhs.hostName.localizedCaseInsensitiveCompare(rhs.hostName) == .orderedAscending
        }
    }

    func load(serverId: String, apiClient: APIClient) async {
        isLoading = true
        errorMessage = nil
        defer {
            isLoading = false
            hasLoaded = true
        }
        do {
            let fetched: ServerTailscaleStatusResponse = try await apiClient.get(
                "/api/servers/\(serverId)/tailscale/status"
            )
            response = fetched
        } catch {
            errorMessage = Self.message(for: error)
        }
    }

    func refresh(serverId: String, apiClient: APIClient) async {
        await load(serverId: serverId, apiClient: apiClient)
    }

    static func message(for error: Error) -> String {
        if case APIError.httpError(let code, let data) = error {
            if let msg = AccountSecurityViewModel.errorMessage(from: data), !msg.isEmpty {
                return msg
            }
            switch code {
            case 403:
                return String(localized: "Tailscale capability is not enabled for this server.")
            case 404:
                return String(localized: "Tailscale status is not available for this server.")
            case 408, 504:
                return String(localized: "The agent did not respond in time.")
            default:
                break
            }
        }
        return AccountSecurityViewModel.message(
            for: error,
            fallback: String(localized: "Failed to load Tailscale status.")
        )
    }
}
