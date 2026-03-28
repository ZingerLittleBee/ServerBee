import SwiftUI

/// A card representing a single server in the servers list.
/// Shows online status, name, IP, and key metric pills (CPU, Memory, OS).
struct ServerCardView: View {
    let server: ServerStatus

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            // Top row: status dot + name
            HStack(spacing: 8) {
                Circle()
                    .fill(server.online ? Color.serverOnline : Color.serverOffline)
                    .frame(width: 10, height: 10)

                Text(server.name)
                    .font(.headline)
                    .lineLimit(1)

                Spacer()

                if let lastActive = server.lastActiveAt, !server.online {
                    Text(Formatters.formatRelativeTime(lastActive))
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            // IP address
            if let ip = server.primaryIP {
                Text(ip)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            // Metric pills
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
        .padding()
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.05), radius: 3, y: 2)
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
