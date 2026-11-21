import SwiftUI
import UIKit

/// Grouped navigation row card surfaced on `ServerOverviewSection` when a server
/// has `CAP_TAILSCALE` enabled and reports a Tailscale interface or persisted IP.
struct ServerTailscaleNavigationCard: View {
    let serverId: String
    let serverName: String
    let tailscaleIpv4: String?
    let dnsName: String?
    let tailnet: String?

    @Environment(\.privacyMode) private var privacyMode

    private var subtitle: String? {
        let parts = [dnsName?.maskingIPs(privacyMode), tailnet?.maskingIPs(privacyMode)]
            .compactMap { $0 }.filter { !$0.isEmpty }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    var body: some View {
        OverviewInfoGroup(title: String(localized: "Tailscale")) {
            NavigationLink(value: ServerNavigationTarget.tailscale(serverId: serverId, serverName: serverName)) {
                HStack(spacing: 12) {
                    IconRowLabel(
                        title: String(localized: "Tailscale"),
                        systemImage: "circle.grid.3x3.fill",
                        color: .cyan,
                        subtitle: subtitle,
                        value: tailscaleIpv4?.maskingIPs(privacyMode)
                    )
                    Image(systemName: "chevron.right")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(.tertiary)
                        .accessibilityHidden(true)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .cardSurface(padding: 0)
            .accessibilityIdentifier("server.tailscale.link")
        }
    }
}

/// Native Tailscale detail view for a single server (`GET /api/servers/{id}/tailscale/status`).
struct ServerTailscaleView: View {
    let serverId: String
    var serverName: String = ""
    var embedded = false

    @Environment(\.apiClient) private var apiClient
    @Environment(\.privacyMode) private var privacyMode
    @State private var viewModel = ServerTailscaleViewModel()
    @State private var copiedToken: String?

    var body: some View {
        Group {
            if embedded {
                mainBody
            } else {
                mainBody
                    .navigationTitle(String(localized: "Tailscale"))
                    .navigationBarTitleDisplayMode(.inline)
            }
        }
        .background(Color(.systemGroupedBackground))
        .refreshable {
            await viewModel.refresh(serverId: serverId, apiClient: apiClient)
        }
        .task {
            if !viewModel.hasLoaded {
                await viewModel.load(serverId: serverId, apiClient: apiClient)
            }
        }
    }

    @ViewBuilder
    private var mainBody: some View {
        if viewModel.isLoading && !viewModel.hasLoaded {
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if let error = viewModel.errorMessage, viewModel.response == nil {
            errorState(error)
        } else if let response = viewModel.response, !response.available, response.status == nil, viewModel.primaryIpv4 == nil {
            unavailableState
        } else {
            statusList
        }
    }
}

extension ServerTailscaleView {
    private var statusList: some View {
        List {
            if let response = viewModel.response, response.cached || !response.online {
                cachedNoticeSection(response: response)
            }
            if let warnings = viewModel.report?.healthWarnings, !warnings.isEmpty {
                healthWarningsSection(warnings)
            }
            nodeSummarySection
            routingSection
            peersSection
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
    }

    private func cachedNoticeSection(response: ServerTailscaleStatusResponse) -> some View {
        Section {
            Label {
                Text(
                    response.online
                        ? String(localized: "Showing cached Tailscale snapshot.")
                        : String(localized: "Agent is offline. Showing last cached Tailscale snapshot.")
                )
                .font(.footnote)
                .foregroundStyle(.secondary)
            } icon: {
                Image(systemName: "clock.arrow.circlepath")
                    .foregroundStyle(Color.warningAmber)
            }
        }
    }

    private func healthWarningsSection(_ warnings: [String]) -> some View {
        Section(String(localized: "Health Warnings")) {
            ForEach(Array(warnings.enumerated()), id: \.offset) { _, warning in
                Label {
                    Text(verbatim: warning.maskingIPs(privacyMode))
                        .font(.footnote)
                } icon: {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(Color.warningAmber)
                }
            }
        }
    }

    private var nodeSummarySection: some View {
        Section {
            if let summary = viewModel.summary {
                LabeledContent(String(localized: "State")) {
                    StatusBadge(
                        text: summary.backendStateLabel,
                        color: summary.online && summary.backendState == "Running" ? .serverOnline : .warningAmber
                    )
                }
            }
            if let ipv4 = viewModel.primaryIpv4 {
                copyableRow(
                    label: String(localized: "Tailscale IPv4"),
                    displayed: ipv4.maskingIPs(privacyMode),
                    raw: ipv4,
                    monospaced: true
                )
            }
            if let ipv6 = viewModel.primaryIpv6 {
                copyableRow(
                    label: String(localized: "Tailscale IPv6"),
                    displayed: ipv6.maskingIPs(privacyMode),
                    raw: ipv6,
                    monospaced: true
                )
            }
            if let dns = viewModel.dnsName {
                copyableRow(
                    label: String(localized: "MagicDNS"),
                    displayed: dns.maskingIPs(privacyMode),
                    raw: dns,
                    monospaced: false
                )
            }
            if let hostname = viewModel.summary?.hostname, !hostname.isEmpty {
                LabeledContent(String(localized: "Hostname"), value: hostname.maskingIPs(privacyMode))
            }
            if let tailnet = viewModel.tailnetName {
                LabeledContent(String(localized: "Tailnet"), value: tailnet.maskingIPs(privacyMode))
            }
            if let derp = viewModel.report?.derpRegion, !derp.isEmpty {
                LabeledContent(String(localized: "DERP Region"), value: derp)
            }
            if let version = viewModel.summary?.version, !version.isEmpty {
                LabeledContent(String(localized: "Version"), value: version)
            }
            if viewModel.summary != nil {
                keyExpiryRow
            }
        } header: {
            Text(String(localized: "Node Status"))
        } footer: {
            if let collectedAt = viewModel.report?.collectedAt {
                Text(String(localized: "Collected \(Formatters.formatRelativeTime(collectedAt))"))
            }
        }
    }

    private var keyExpiryRow: some View {
        LabeledContent(String(localized: "Key Expiry")) {
            if let expiry = TailscaleFormatting.validTimestamp(viewModel.summary?.keyExpiry) {
                let isSoon = Self.isExpiringSoon(expiry)
                Text(Formatters.formatRelativeTime(expiry, allowsFuture: true))
                    .foregroundStyle(isSoon ? Color.warningAmber : Color.secondary)
                    .monospacedDigit()
            } else {
                Text(String(localized: "No expiry"))
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var routingSection: some View {
        Section(String(localized: "Routing & Access")) {
            if let summary = viewModel.summary {
                LabeledContent(String(localized: "Exit Node")) {
                    if summary.exitNode {
                        StatusBadge(text: summary.exitNodeStatusLabel, color: .serverOnline)
                    } else if summary.exitNodeOption {
                        StatusBadge(text: summary.exitNodeStatusLabel, color: .accentColor)
                    } else {
                        Text(summary.exitNodeStatusLabel)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            let routes = viewModel.report?.advertisedRoutes ?? []
            if routes.isEmpty {
                LabeledContent(String(localized: "Subnet Routes"), value: String(localized: "None"))
            } else {
                VStack(alignment: .leading, spacing: 6) {
                    Text(String(localized: "Subnet Routes"))
                        .font(.subheadline)
                    FlexibleWrap(items: routes) { route in
                        Chip(text: route.maskingIPs(privacyMode), color: .accentColor)
                            .contextMenu {
                                Button {
                                    copyValue(route, token: "route:\(route)")
                                } label: {
                                    Label(String(localized: "Copy"), systemImage: "doc.on.doc")
                                }
                            }
                    }
                }
                .padding(.vertical, 2)
            }
            let tags = viewModel.report?.tags ?? []
            if !tags.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Text(String(localized: "ACL Tags"))
                        .font(.subheadline)
                    FlexibleWrap(items: tags) { tag in
                        Chip(text: tag.maskingIPs(privacyMode), systemImage: "tag", color: .secondary)
                    }
                }
                .padding(.vertical, 2)
            }
        }
    }

    private var peersSection: some View {
        let peers = viewModel.sortedPeers
        let onlineCount = peers.filter(\.online).count
        return Section {
            if peers.isEmpty {
                Text(String(localized: "No Tailscale peers reported."))
                    .foregroundStyle(.secondary)
            } else {
                ForEach(peers) { peer in
                    TailscalePeerRow(peer: peer)
                }
            }
        } header: {
            HStack {
                Text(String(localized: "Peers"))
                Spacer()
                if !peers.isEmpty {
                    Text(String(localized: "\(onlineCount)/\(peers.count) online"))
                        .font(.caption)
                        .monospacedDigit()
                }
            }
        }
    }

    private func copyableRow(label: String, displayed: String, raw: String, monospaced: Bool) -> some View {
        let token = "row:\(label)"
        return Button {
            copyValue(raw, token: token)
        } label: {
            LabeledContent(label) {
                if copiedToken == token {
                    Label(String(localized: "Copied"), systemImage: "checkmark")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(Color.serverOnline)
                } else {
                    HStack(spacing: 5) {
                        Text(verbatim: displayed)
                            .font(monospaced ? .subheadline.monospaced() : .subheadline)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.trailing)
                        Image(systemName: "doc.on.doc")
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                            .accessibilityHidden(true)
                    }
                }
            }
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button {
                copyValue(raw, token: token)
            } label: {
                Label(String(localized: "Copy"), systemImage: "doc.on.doc")
            }
        }
    }

    private func copyValue(_ raw: String, token: String) {
        UIPasteboard.general.string = raw
        copiedToken = token
        Task {
            try? await Task.sleep(for: .seconds(1.5))
            if copiedToken == token {
                copiedToken = nil
            }
        }
    }

    private var unavailableState: some View {
        ContentUnavailableView {
            Label(String(localized: "Tailscale unavailable"), systemImage: "circle.grid.3x3")
        } description: {
            Text(String(localized: "Tailscale is not active on this server."))
        } actions: {
            Button(String(localized: "Try again")) {
                Task { await viewModel.load(serverId: serverId, apiClient: apiClient) }
            }
            .buttonStyle(.borderedProminent)
        }
    }

    private func errorState(_ message: String) -> some View {
        ContentUnavailableView {
            Label(String(localized: "Tailscale unavailable"), systemImage: "circle.grid.3x3")
        } description: {
            Text(message)
        } actions: {
            Button(String(localized: "Try again")) {
                Task { await viewModel.load(serverId: serverId, apiClient: apiClient) }
            }
            .buttonStyle(.borderedProminent)
        }
    }

    private static func isExpiringSoon(_ isoString: String) -> Bool {
        guard let date = ISO8601DateFormatter.shared.date(from: isoString) else { return false }
        return date.timeIntervalSinceNow < 7 * 86_400
    }
}

/// One peer row inside `ServerTailscaleView`.
private struct TailscalePeerRow: View {
    let peer: TailscalePeerInfo
    @Environment(\.privacyMode) private var privacyMode

    private var modeColor: Color {
        switch peer.connectionMode.lowercased() {
        case "direct": .serverOnline
        case "derp": .warningAmber
        case "peer_relay": .blue
        default: .secondary
        }
    }

    private var trafficText: String {
        let rx = Formatters.formatBytes(Int64(clamping: peer.rxBytes))
        let tx = Formatters.formatBytes(Int64(clamping: peer.txBytes))
        return "↓ \(rx) · ↑ \(tx)"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Circle()
                    .fill(peer.online ? Color.serverOnline : Color.serverOffline)
                    .frame(width: 8, height: 8)
                    .accessibilityHidden(true)
                Text(verbatim: peer.hostName.maskingIPs(privacyMode))
                    .font(.callout.weight(.semibold))
                    .lineLimit(1)
                if let os = peer.os, !os.isEmpty {
                    Text(verbatim: os)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                if peer.exitNode {
                    StatusBadge(text: String(localized: "Exit Node"), color: .serverOnline)
                }
                Spacer(minLength: 6)
                StatusBadge(text: peer.connectionModeLabel, color: modeColor)
            }

            if let ip = peer.primaryIp {
                HStack(spacing: 6) {
                    Text(verbatim: ip.maskingIPs(privacyMode))
                        .font(.footnote.monospaced())
                        .foregroundStyle(.secondary)
                    if let dns = peer.cleanDnsName, dns != peer.hostName {
                        Text(verbatim: "· \(dns.maskingIPs(privacyMode))")
                            .font(.footnote)
                            .foregroundStyle(.tertiary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }
            }

            if let endpoint = peer.curAddr, !endpoint.isEmpty {
                Label {
                    Text(verbatim: endpoint.maskingIPs(privacyMode))
                        .font(.caption.monospaced())
                } icon: {
                    Image(systemName: "arrow.left.and.right")
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }

            HStack(spacing: 8) {
                Text(verbatim: trafficText)
                    .monospacedDigit()
                Spacer(minLength: 6)
                if let handshake = peer.validLastHandshake {
                    Text(String(localized: "Handshake \(Formatters.formatRelativeTime(handshake))"))
                } else if !peer.online, let seen = peer.validLastSeen {
                    Text(String(localized: "Seen \(Formatters.formatRelativeTime(seen))"))
                }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        .padding(.vertical, 3)
        .contextMenu {
            if let ip = peer.primaryIp {
                Button {
                    UIPasteboard.general.string = ip
                } label: {
                    Label(String(localized: "Copy Tailscale IP"), systemImage: "doc.on.doc")
                }
            }
            if let dns = peer.cleanDnsName {
                Button {
                    UIPasteboard.general.string = dns
                } label: {
                    Label(String(localized: "Copy MagicDNS"), systemImage: "link")
                }
            }
            if let endpoint = peer.curAddr, !endpoint.isEmpty {
                Button {
                    UIPasteboard.general.string = endpoint
                } label: {
                    Label(String(localized: "Copy Endpoint"), systemImage: "network")
                }
            }
        }
    }
}
