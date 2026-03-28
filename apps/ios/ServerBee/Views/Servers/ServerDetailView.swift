import SwiftUI

/// Detailed view for a single server showing status, metrics, and a link to history charts.
struct ServerDetailView: View {
    let server: ServerStatus

    private let columns = [
        GridItem(.flexible()),
        GridItem(.flexible()),
    ]

    var body: some View {
        ScrollView {
            VStack(spacing: 20) {
                headerSection
                metricsGrid
                historyButton
            }
            .padding()
        }
        .background(Color(.systemGroupedBackground))
        .navigationTitle(server.name)
        .navigationBarTitleDisplayMode(.inline)
    }

    // MARK: - Header Section

    private var headerSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            // Status badge and name
            HStack(spacing: 8) {
                statusBadge
                Spacer()
                if let uptime = server.uptime {
                    Label(Formatters.formatUptime(uptime), systemImage: "clock")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            // IP Address
            if let ip = server.primaryIP {
                Label(ip, systemImage: "network")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }

            // OS and CPU
            HStack(spacing: 16) {
                if let os = server.os {
                    Label(os, systemImage: "desktopcomputer")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                if let cpuName = server.cpuName {
                    Label(cpuName, systemImage: "cpu")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }

            // Location
            if let location = server.location {
                Label(location, systemImage: "location")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding()
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
    }

    private var statusBadge: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(server.online ? Color.serverOnline : Color.serverOffline)
                .frame(width: 10, height: 10)
            Text(server.online ? String(localized: "Online") : String(localized: "Offline"))
                .font(.subheadline.bold())
                .foregroundStyle(server.online ? Color.serverOnline : Color.serverOffline)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 5)
        .background(
            (server.online ? Color.serverOnline : Color.serverOffline).opacity(0.1)
        )
        .clipShape(Capsule())
    }

    // MARK: - Metrics Grid

    private var metricsGrid: some View {
        LazyVGrid(columns: columns, spacing: 12) {
            // CPU
            MetricCardView(
                label: String(localized: "CPU"),
                value: Formatters.formatPercentage(server.cpuUsage),
                subtitle: server.cpuName,
                valueColor: server.cpuUsage.map { Formatters.cpuColor(for: $0) } ?? .primary
            )

            // Memory
            MetricCardView(
                label: String(localized: "Memory"),
                value: Formatters.formatPercentage(server.memoryPercent),
                subtitle: Formatters.formatBytesRatio(used: server.memoryUsed, total: server.memoryTotal),
                valueColor: server.memoryPercent.map { Formatters.usageColor(for: $0) } ?? .primary
            )

            // Disk
            MetricCardView(
                label: String(localized: "Disk"),
                value: Formatters.formatPercentage(server.diskPercent),
                subtitle: Formatters.formatBytesRatio(used: server.diskUsed, total: server.diskTotal),
                valueColor: server.diskPercent.map { Formatters.usageColor(for: $0) } ?? .primary
            )

            // Network In
            MetricCardView(
                label: String(localized: "Network In"),
                value: Formatters.formatSpeed(server.networkIn),
                valueColor: .networkColor
            )

            // Network Out
            MetricCardView(
                label: String(localized: "Network Out"),
                value: Formatters.formatSpeed(server.networkOut),
                valueColor: .networkColor
            )

            // Load
            MetricCardView(
                label: String(localized: "Load"),
                value: server.load1.map { String(format: "%.2f", $0) } ?? "-",
                subtitle: loadSubtitle
            )

            // Processes
            MetricCardView(
                label: String(localized: "Processes"),
                value: server.processCount.map { "\($0)" } ?? "-"
            )

            // TCP / UDP
            MetricCardView(
                label: String(localized: "TCP / UDP"),
                value: tcpUdpValue
            )
        }
    }

    private var loadSubtitle: String? {
        guard let l5 = server.load5, let l15 = server.load15 else { return nil }
        return String(format: "%.2f / %.2f", l5, l15)
    }

    private var tcpUdpValue: String {
        let tcp = server.tcpCount.map { "\($0)" } ?? "-"
        let udp = server.udpCount.map { "\($0)" } ?? "-"
        return "\(tcp) / \(udp)"
    }

    // MARK: - History Button

    private var historyButton: some View {
        NavigationLink {
            MetricsHistoryView(serverId: server.id)
        } label: {
            HStack {
                Label(String(localized: "View History"), systemImage: "chart.xyaxis.line")
                Spacer()
                Image(systemName: "chevron.right")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .padding()
            .background(Color(.systemBackground))
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
        }
        .buttonStyle(.plain)
    }
}

#Preview {
    NavigationStack {
        ServerDetailView(
            server: ServerStatus(
                id: "1",
                name: "Production Server",
                online: true,
                cpuUsage: 45.2,
                memoryTotal: 17_179_869_184,
                memoryUsed: 12_516_925_440,
                diskTotal: 512_110_190_592,
                diskUsed: 307_266_114_355,
                networkIn: 1_048_576,
                networkOut: 524_288,
                load1: 1.25,
                load5: 1.10,
                load15: 0.95,
                processCount: 312,
                tcpCount: 45,
                udpCount: 12,
                uptime: 345_600,
                os: "Ubuntu 22.04",
                cpuName: "Intel i7-12700K",
                ipv4: "192.168.1.100",
                region: "Virginia",
                country: "US"
            )
        )
    }
}
