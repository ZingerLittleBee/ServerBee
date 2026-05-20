import SwiftUI

/// A card representing a single server in the servers list.
/// Shows online status, name, IP, and key metric pills (CPU, Memory, OS).
struct ServerCardView: View, Equatable {
    nonisolated static func == (lhs: ServerCardView, rhs: ServerCardView) -> Bool {
        lhs.server.id == rhs.server.id &&
            lhs.server.isOnline == rhs.server.isOnline &&
            lhs.server.cpuUsage == rhs.server.cpuUsage &&
            lhs.server.memoryUsed == rhs.server.memoryUsed &&
            lhs.server.name == rhs.server.name &&
            lhs.server.lastActiveAt == rhs.server.lastActiveAt &&
            lhs.server.primaryIP == rhs.server.primaryIP &&
            lhs.server.os == rhs.server.os
    }

    let server: ServerStatus

    @ScaledMetric(relativeTo: .body) private var cardPad: CGFloat = 14
    @ScaledMetric(relativeTo: .caption2) private var dotSize: CGFloat = 10

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Circle()
                    .fill(server.isOnline ? Color.serverOnline : Color.serverOffline)
                    .frame(width: dotSize, height: dotSize)
                    .accessibilityHidden(true)

                Text(server.name)
                    .font(.headline)
                    .lineLimit(1)

                Spacer()

                if let lastActive = server.lastActiveAt, !server.isOnline {
                    Text(Formatters.formatRelativeTime(lastActive))
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            if let ip = server.primaryIP {
                Text(ip)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            HStack(spacing: 8) {
                MetricPill(
                    label: String(localized: "CPU"),
                    value: Formatters.formatPercentage(server.cpuUsage),
                    color: .cpuColor
                )

                MetricPill(
                    label: String(localized: "MEM"),
                    value: server.memoryUsed.map { Formatters.formatBytes($0) } ?? "-",
                    color: .memoryColor
                )

                if let os = server.os {
                    MetricPill(
                        label: String(localized: "OS"),
                        value: os,
                        color: .secondary
                    )
                }
            }
        }
        .padding(cardPad)
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.05), radius: 3, y: 2)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(accessibilityLabelText))
    }

    private var accessibilityLabelText: String {
        let status = server.isOnline
            ? String(localized: "Online")
            : String(localized: "Offline")
        let cpu = Formatters.formatPercentage(server.cpuUsage)
        let mem = server.memoryUsed.map { Formatters.formatBytes($0) } ?? "-"
        return String(
            format: String(localized: "%1$@, %2$@, CPU %3$@, memory %4$@"),
            server.name, status, cpu, mem
        )
    }
}

// MARK: - Metric Pill

/// A small pill showing a label and value, used at the bottom of the server card.
private struct MetricPill: View {
    let label: String
    let value: String
    let color: Color

    var body: some View {
        HStack(spacing: 4) {
            Text(label)
                .font(.caption2.bold())
                .foregroundStyle(color)
            Text(value)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(color.opacity(0.1))
        .clipShape(Capsule())
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(label))
        .accessibilityValue(Text(value))
    }
}

#Preview {
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
    .padding()
    .background(Color(.systemGroupedBackground))
}
