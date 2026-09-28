import SwiftUI

/// A single server row in the servers list: status dot, name, live network
/// rate (or a high-CPU warning), IP / OS, and inline CPU / memory / disk bars.
/// Offline servers collapse to the name plus an "Offline · <relative time>" label.
struct ServerCardView: View, Equatable {
    nonisolated static func == (lhs: ServerCardView, rhs: ServerCardView) -> Bool {
        lhs.server.id == rhs.server.id &&
            lhs.server.isOnline == rhs.server.isOnline &&
            lhs.server.name == rhs.server.name &&
            lhs.server.cpuUsage == rhs.server.cpuUsage &&
            lhs.server.memoryUsed == rhs.server.memoryUsed &&
            lhs.server.memoryTotal == rhs.server.memoryTotal &&
            lhs.server.diskUsed == rhs.server.diskUsed &&
            lhs.server.diskTotal == rhs.server.diskTotal &&
            lhs.server.networkIn == rhs.server.networkIn &&
            lhs.server.networkOut == rhs.server.networkOut &&
            lhs.server.lastActiveAt == rhs.server.lastActiveAt &&
            lhs.server.primaryIP == rhs.server.primaryIP &&
            lhs.server.os == rhs.server.os
    }

    /// CPU percentage at or above which an online row is flagged as high load.
    static let highCPUThreshold: Double = 85

    let server: ServerStatus

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.privacyMode) private var privacyMode
    @ScaledMetric(relativeTo: .headline) private var dotSize: CGFloat = 9

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            Circle()
                .fill(statusColor)
                .frame(width: dotSize, height: dotSize)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 6) {
                VStack(alignment: .leading, spacing: 1) {
                    titleLine
                    if let details = detailsText {
                        Text(verbatim: details)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
                if server.isOnline {
                    usageBars
                }
            }
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(accessibilityLabelText))
        .accessibilityValue(Text(accessibilityValueText))
    }
}

// MARK: - Subviews

private extension ServerCardView {
    var isHighLoad: Bool {
        server.isOnline && (server.cpuUsage ?? 0) >= Self.highCPUThreshold
    }

    var statusColor: Color {
        if !server.isOnline { return .serverOffline }
        return isHighLoad ? .warningAmber : .serverOnline
    }

    /// Name on the leading edge, status/rate on the trailing edge. Stacks
    /// vertically at accessibility sizes so neither side truncates to nothing.
    var titleLine: some View {
        let layout = dynamicTypeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 2))
            : AnyLayout(HStackLayout(alignment: .firstTextBaseline, spacing: 8))
        return layout {
            Text(server.name)
                .font(.headline)
                .foregroundStyle(server.isOnline ? .primary : .secondary)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            trailingStatus
                .lineLimit(1)
                .layoutPriority(1)
        }
    }

    @ViewBuilder
    var trailingStatus: some View {
        if !server.isOnline {
            Text(verbatim: offlineText)
                .font(.footnote)
                .foregroundStyle(Color.serverOffline)
        } else if isHighLoad {
            Text(String(localized: "CPU high"))
                .font(.footnote.weight(.semibold))
                .foregroundStyle(Color.warningAmber)
        } else if let rate = rateText {
            Text(verbatim: rate)
                .font(.footnote)
                .foregroundStyle(.secondary)
                .monospacedDigit()
        }
    }

    /// Three equal columns of usage bars; a single column at accessibility sizes.
    var usageBars: some View {
        let layout = dynamicTypeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 6))
            : AnyLayout(HStackLayout(alignment: .top, spacing: 8))
        return layout {
            InlineUsageBar(
                label: String(localized: "CPU"),
                percent: server.cpuUsage,
                color: .cpuColor,
                warnAt: Self.highCPUThreshold
            )
            .frame(maxWidth: .infinity, alignment: .leading)
            InlineUsageBar(label: String(localized: "MEM"), percent: server.memoryPercent, color: .memoryColor)
                .frame(maxWidth: .infinity, alignment: .leading)
            InlineUsageBar(label: String(localized: "DISK"), percent: server.diskPercent, color: .diskColor)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    /// "Offline · 2 hr. ago", or just "Offline" when the last-active time is unknown.
    var offlineText: String {
        let offline = String(localized: "Offline")
        guard let lastActive = server.lastActiveAt else { return offline }
        return "\(offline) · \(Formatters.formatRelativeTime(lastActive))"
    }

    /// "↓4.8 ↑1.2 MB/s", or `nil` when the agent has not reported network rates.
    var rateText: String? {
        guard server.networkIn != nil || server.networkOut != nil else { return nil }
        return ServerListRateFormat.pair(down: server.networkIn ?? 0, up: server.networkOut ?? 0)
    }

    /// Primary IP and OS joined, e.g. "192.168.1.100 · Ubuntu 22.04".
    var detailsText: String? {
        let parts = [server.primaryIP?.maskingIPs(privacyMode), server.os].compactMap { $0 }.filter { !$0.isEmpty }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    // MARK: Accessibility

    var accessibilityLabelText: String {
        guard server.isOnline else { return server.name }
        let cpu = Formatters.formatPercentage(server.cpuUsage)
        let mem = Formatters.formatPercentage(server.memoryPercent)
        return String(
            format: String(localized: "%1$@, %2$@, CPU %3$@, memory %4$@"),
            server.name, String(localized: "Online"), cpu, mem
        )
    }

    var accessibilityValueText: String {
        var parts: [String] = []
        if server.isOnline {
            if isHighLoad { parts.append(String(localized: "CPU high")) }
            parts.append("\(String(localized: "Disk")) \(Formatters.formatPercentage(server.diskPercent))")
            if server.networkIn != nil || server.networkOut != nil {
                parts.append("\(String(localized: "Download")) \(Formatters.formatSpeed(server.networkIn ?? 0))")
                parts.append("\(String(localized: "Upload")) \(Formatters.formatSpeed(server.networkOut ?? 0))")
            }
        } else {
            parts.append(String(localized: "Offline"))
            if let lastActive = server.lastActiveAt {
                parts.append(String(localized: "Last seen \(Formatters.formatRelativeTime(lastActive))"))
            }
        }
        if let details = detailsText { parts.append(details) }
        return parts.joined(separator: ", ")
    }
}

// MARK: - Rate formatting

/// Formats live byte rates for the servers list with the shared byte units.
/// A down/up pair shares the larger value's unit so it reads as one figure.
enum ServerListRateFormat {
    /// Splits a rate into a display number and unit, e.g. `("38", "MB/s")`.
    static func split(_ bytesPerSec: Int64) -> (value: String, unit: String) {
        let value = Double(max(bytesPerSec, 0))
        let index = Formatters.byteUnitIndex(for: value)
        return (Formatters.byteNumber(value, unitIndex: index), unit(index))
    }

    /// Down/up pair in one shared unit, e.g. `"↓4.8 ↑1.2 MB/s"`.
    static func pair(down: Int64, up: Int64) -> String {
        let downValue = Double(max(down, 0))
        let upValue = Double(max(up, 0))
        let index = Formatters.byteUnitIndex(for: max(downValue, upValue))
        let downText = Formatters.byteNumber(downValue, unitIndex: index)
        let upText = Formatters.byteNumber(upValue, unitIndex: index)
        return "↓\(downText) ↑\(upText) \(unit(index))"
    }

    private static func unit(_ index: Int) -> String {
        "\(Formatters.byteUnits[index])/s"
    }
}

#Preview {
    List {
        ServerCardView(
            server: ServerStatus(
                id: "1",
                name: "Production Web Server",
                online: true,
                cpuUsage: 45.2,
                memoryTotal: 17_179_869_184,
                memoryUsed: 12_516_925_440,
                os: "Ubuntu 22.04",
                ipv4: "192.168.1.100"
            )
        )
        ServerCardView(server: ServerStatus(id: "2", name: "Busy Relay", online: true, cpuUsage: 91))
        ServerCardView(server: ServerStatus(id: "3", name: "Lab Box", online: false))
    }
    .listStyle(.insetGrouped)
}
