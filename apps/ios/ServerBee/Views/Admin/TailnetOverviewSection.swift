import SwiftUI
import UIKit

/// Live Tailnet overview stats and device inventory section rendered inside
/// `TailscaleAdminView` (`GET /api/tailscale/overview`).
struct TailnetOverviewSection: View {
    let overview: TailnetOverviewResponse?
    let devices: [TailnetDeviceItem]

    @Environment(\.privacyMode) private var privacyMode

    var body: some View {
        if let overview {
            if !overview.configured {
                Section(String(localized: "Tailnet Overview")) {
                    Text(String(localized: "Configure and enable a Tailscale or Headscale API credential above to view Tailnet devices."))
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            } else {
                summarySection(overview)
                devicesSection(overview)
            }
        }
    }

    private func summarySection(_ overview: TailnetOverviewResponse) -> some View {
        Section {
            LabeledContent(
                String(localized: "Provider"),
                value: TailscaleProvider.displayName(for: overview.provider)
            )
            if !overview.tailnet.isEmpty {
                LabeledContent(
                    String(localized: "Tailnet"),
                    value: overview.tailnet.maskingIPs(privacyMode)
                )
            }
            LabeledContent(
                String(localized: "Online Devices"),
                value: "\(overview.onlineDevices) / \(overview.totalDevices)"
            )
            .monospacedDigit()
            LabeledContent(
                String(localized: "Linked Servers"),
                value: "\(overview.linkedServers)"
            )
            .monospacedDigit()
            LabeledContent(String(localized: "Expiring Keys")) {
                Text(verbatim: "\(overview.expiringKeysCount)")
                    .foregroundStyle(overview.expiringKeysCount > 0 ? Color.warningAmber : Color.secondary)
                    .monospacedDigit()
            }
            if let serverUrl = overview.serverTailnetUrl, !serverUrl.isEmpty {
                LabeledContent(String(localized: "Server Tailnet URL")) {
                    Text(verbatim: serverUrl.maskingIPs(privacyMode))
                        .font(.footnote.monospaced())
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.trailing)
                }
                .contextMenu {
                    Button {
                        UIPasteboard.general.string = serverUrl
                    } label: {
                        Label(String(localized: "Copy"), systemImage: "doc.on.doc")
                    }
                }
            }
            if let err = overview.error, !err.isEmpty {
                Label {
                    Text(verbatim: err.maskingIPs(privacyMode))
                        .font(.footnote)
                } icon: {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(Color.serverOffline)
                }
            }
        } header: {
            Text(String(localized: "Tailnet Overview"))
        } footer: {
            if let fetchedAt = overview.fetchedAt {
                Text(String(localized: "Updated \(Formatters.formatRelativeTime(fetchedAt))"))
            }
        }
    }

    private func devicesSection(_ overview: TailnetOverviewResponse) -> some View {
        Section {
            if devices.isEmpty {
                Text(String(localized: "No devices reported by the control plane."))
                    .foregroundStyle(.secondary)
            } else {
                ForEach(devices) { device in
                    if let serverId = device.matchedServerId {
                        NavigationLink {
                            ServerDetailView(serverId: serverId)
                        } label: {
                            TailnetDeviceRow(device: device)
                        }
                    } else {
                        TailnetDeviceRow(device: device)
                    }
                }
            }
        } header: {
            HStack {
                Text(String(localized: "Tailnet Devices"))
                Spacer()
                Text(verbatim: "\(overview.onlineDevices)/\(overview.totalDevices)")
                    .font(.caption)
                    .monospacedDigit()
            }
        }
    }
}

private struct TailnetDeviceRow: View {
    let device: TailnetDeviceItem
    @Environment(\.privacyMode) private var privacyMode

    private var maskedAddresses: String {
        device.addresses.map { $0.maskingIPs(privacyMode) }.joined(separator: " · ")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Circle()
                    .fill(device.online ? Color.serverOnline : Color.serverOffline)
                    .frame(width: 8, height: 8)
                    .accessibilityHidden(true)
                Text(verbatim: device.cleanName.maskingIPs(privacyMode))
                    .font(.callout.weight(.semibold))
                    .lineLimit(1)
                if !device.authorized {
                    StatusBadge(text: String(localized: "Unauthorized"), color: .warningAmber)
                }
                Spacer(minLength: 6)
                if let serverName = device.matchedServerName ?? (device.matchedServerId != nil ? String(localized: "Linked") : nil) {
                    StatusBadge(
                        text: serverName.maskingIPs(privacyMode),
                        color: .accentColor,
                        systemImage: "server.rack"
                    )
                }
            }

            if !device.addresses.isEmpty {
                Text(verbatim: maskedAddresses)
                    .font(.footnote.monospaced())
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }

            HStack(spacing: 8) {
                let meta = [device.os, device.clientVersion.map { "v\($0)" }]
                    .compactMap { $0 }
                    .filter { !$0.isEmpty }
                    .joined(separator: " · ")
                if !meta.isEmpty {
                    Text(verbatim: meta)
                }
                Spacer(minLength: 6)
                expiryOrSeenLabel
            }
            .font(.caption)
            .foregroundStyle(.secondary)

            if !device.tags.isEmpty {
                FlexibleWrap(items: device.tags) { tag in
                    Chip(text: tag.maskingIPs(privacyMode), systemImage: "tag", color: .secondary)
                }
            }
        }
        .padding(.vertical, 3)
        .contextMenu {
            if let ip = device.primaryAddress {
                Button {
                    UIPasteboard.general.string = ip
                } label: {
                    Label(String(localized: "Copy Tailscale IP"), systemImage: "doc.on.doc")
                }
            }
            Button {
                UIPasteboard.general.string = device.cleanName
            } label: {
                Label(String(localized: "Copy Hostname"), systemImage: "link")
            }
        }
    }

    @ViewBuilder
    private var expiryOrSeenLabel: some View {
        if !device.online, let seen = device.validLastSeen {
            Text(String(localized: "Seen \(Formatters.formatRelativeTime(seen))"))
        } else if device.keyExpiryDisabled {
            Text(String(localized: "No key expiry"))
        } else if let expires = device.validExpires {
            Text(String(localized: "Expires \(Formatters.formatRelativeTime(expires, allowsFuture: true))"))
        }
    }
}
