import SwiftUI

/// The "Overview" tab of the server detail screen: status header, live metric
/// grid, system info, capabilities, and billing — composed from the live WS
/// status (metrics) and the REST config (static metadata).
struct ServerOverviewSection: View {
    let serverId: String
    let live: ServerStatus?
    let config: ServerConfig?
    let groupName: String?
    let capabilities: CapabilitySet
    let isAdmin: Bool

    @Environment(\.dismiss) private var dismiss

    private let columns = [GridItem(.flexible()), GridItem(.flexible())]

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                if isPending {
                    pendingBanner
                }
                statusHeader
                if hasAnyMetric {
                    metricsGrid
                }
                systemInfoCard
                capabilitiesCard
                if hasBilling {
                    billingCard
                }
                if isAdmin {
                    ServerLifecycleCard(
                        serverId: serverId,
                        config: config,
                        capabilities: capabilities,
                        isOnline: isOnline,
                        isPending: isPending,
                        onDeleted: { dismiss() }
                    )
                }
            }
            .padding()
        }
        .background(Color(.systemGroupedBackground))
    }

    // MARK: - Derived

    private var isOnline: Bool { live?.isOnline ?? false }
    private var isPending: Bool { (config?.hasToken == false) || (live?.hasToken == false) }

    private var hasAnyMetric: Bool {
        guard let s = live else { return false }
        return s.cpuUsage != nil || s.memoryUsed != nil || s.diskUsed != nil || s.load1 != nil
    }

    private var hasBilling: Bool {
        config?.price != nil || config?.billingCycle != nil || config?.trafficLimit != nil
    }

    // MARK: - Pending enrollment

    private var pendingBanner: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "person.badge.clock")
                .foregroundStyle(Color.warningAmber)
            VStack(alignment: .leading, spacing: 2) {
                Text(String(localized: "Pending enrollment"))
                    .font(.subheadline.bold())
                Text(String(localized: "This server is waiting for its agent to connect with an enrollment code."))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.warningAmber.opacity(0.12))
        .clipShape(RoundedRectangle(cornerRadius: 12))
    }

    // MARK: - Status header

    private var statusHeader: some View {
        SectionCard {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 10) {
                    if let flag = CountryFlag.emoji(for: config?.countryCode ?? live?.country) {
                        Text(flag).font(.title2)
                    }
                    StatusPill(isOnline: isOnline)
                    Spacer()
                    if let uptime = live?.uptime, isOnline {
                        Label(Formatters.formatUptime(uptime), systemImage: "clock")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }

                if let ip = config?.ipv4 ?? live?.ipv4 ?? config?.ipv6 ?? live?.ipv6 {
                    Label(ip, systemImage: "network")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }

                HStack(spacing: 8) {
                    if let groupName {
                        Chip(text: groupName, systemImage: "folder", color: .brandAccent)
                    }
                    if let tags = live?.tags, !tags.isEmpty {
                        ForEach(tags.prefix(4), id: \.self) { tag in
                            Chip(text: tag, systemImage: "tag")
                        }
                    }
                }

                if !isOnline, let last = live?.lastActiveAt {
                    Text(String(format: String(localized: "Last seen %@"), Formatters.formatRelativeTime(last)))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    // MARK: - Metrics grid

    private var metricsGrid: some View {
        LazyVGrid(columns: columns, spacing: 12) {
            ForEach(metricTiles) { tile in
                MetricCardView(
                    label: tile.label,
                    value: tile.value,
                    subtitle: tile.subtitle,
                    valueColor: tile.color
                )
            }
        }
    }

    fileprivate struct Tile: Identifiable {
        let id: String
        let label: String
        let value: String
        var subtitle: String?
        var color: Color = .primary
    }
}

// MARK: - Metric tiles

private extension ServerOverviewSection {
    var metricTiles: [Tile] {
        guard let s = live else { return [] }
        var tiles: [Tile] = []

        tiles.append(Tile(
            id: "cpu",
            label: String(localized: "CPU"),
            value: Formatters.formatPercentage(s.cpuUsage),
            subtitle: s.cpuName,
            color: s.cpuUsage.map { Formatters.cpuColor(for: $0) } ?? .primary
        ))
        tiles.append(Tile(
            id: "mem",
            label: String(localized: "Memory"),
            value: Formatters.formatPercentage(s.memoryPercent),
            subtitle: Formatters.formatBytesRatio(used: s.memoryUsed, total: s.memoryTotal),
            color: s.memoryPercent.map { Formatters.usageColor(for: $0) } ?? .primary
        ))
        if let swapTotal = s.swapTotal, swapTotal > 0 {
            tiles.append(Tile(
                id: "swap",
                label: String(localized: "Swap"),
                value: Formatters.formatPercentage(s.swapPercent),
                subtitle: Formatters.formatBytesRatio(used: s.swapUsed, total: s.swapTotal),
                color: s.swapPercent.map { Formatters.usageColor(for: $0) } ?? .primary
            ))
        }
        tiles.append(Tile(
            id: "disk",
            label: String(localized: "Disk"),
            value: Formatters.formatPercentage(s.diskPercent),
            subtitle: Formatters.formatBytesRatio(used: s.diskUsed, total: s.diskTotal),
            color: s.diskPercent.map { Formatters.usageColor(for: $0) } ?? .primary
        ))
        tiles.append(Tile(
            id: "load",
            label: String(localized: "Load"),
            value: s.load1.map { String(format: "%.2f", $0) } ?? "—",
            subtitle: loadSubtitle(s)
        ))
        tiles.append(Tile(
            id: "proc",
            label: String(localized: "Processes"),
            value: s.processCount.map { "\($0)" } ?? "—"
        ))
        tiles.append(Tile(
            id: "conn",
            label: String(localized: "TCP / UDP"),
            value: "\(s.tcpCount.map(String.init) ?? "—") / \(s.udpCount.map(String.init) ?? "—")"
        ))
        tiles.append(Tile(
            id: "net",
            label: String(localized: "Network"),
            value: "↓ \(Formatters.formatSpeed(s.networkIn))",
            subtitle: "↑ \(Formatters.formatSpeed(s.networkOut))",
            color: .networkColor
        ))
        if s.diskReadPerSec != nil || s.diskWritePerSec != nil {
            tiles.append(Tile(
                id: "diskio",
                label: String(localized: "Disk I/O"),
                value: "R \(Formatters.formatSpeed(s.diskReadPerSec))",
                subtitle: "W \(Formatters.formatSpeed(s.diskWritePerSec))",
                color: .diskColor
            ))
        }
        if let inT = s.netInTransfer, let outT = s.netOutTransfer {
            tiles.append(Tile(
                id: "transfer",
                label: String(localized: "Transfer"),
                value: "↓ \(Formatters.formatBytes(inT))",
                subtitle: "↑ \(Formatters.formatBytes(outT))"
            ))
        }
        return tiles
    }

    private func loadSubtitle(_ s: ServerStatus) -> String? {
        guard let l5 = s.load5, let l15 = s.load15 else { return nil }
        return String(format: "%.2f / %.2f", l5, l15)
    }

    // MARK: - System info

    private var systemInfoCard: some View {
        SectionCard(String(localized: "System"), systemImage: "cpu") {
            VStack(spacing: 8) {
                DetailRow(label: String(localized: "OS"), value: config?.os ?? live?.os)
                DetailRow(label: String(localized: "Kernel"), value: config?.kernelVersion)
                DetailRow(label: String(localized: "CPU"), value: config?.cpuName ?? live?.cpuName)
                DetailRow(label: String(localized: "Cores"), value: (config?.cpuCores ?? live?.cpuCores).map { "\($0)" })
                DetailRow(label: String(localized: "Architecture"), value: config?.cpuArch)
                DetailRow(label: String(localized: "Virtualization"), value: config?.virtualization)
                DetailRow(label: String(localized: "Agent"), value: config?.agentVersion)
                DetailRow(label: String(localized: "IPv4"), value: config?.ipv4 ?? live?.ipv4, monospaced: true)
                DetailRow(label: String(localized: "IPv6"), value: config?.ipv6 ?? live?.ipv6, monospaced: true)
                if let location = locationText {
                    DetailRow(label: String(localized: "Region"), value: location)
                }
            }
        }
    }

    private var locationText: String? {
        let region = config?.region ?? live?.region
        let country = config?.countryCode ?? live?.country
        switch (region, country) {
        case let (r?, c?): return "\(r), \(c)"
        case let (r?, nil): return r
        case let (nil, c?): return c
        default: return nil
        }
    }

    // MARK: - Capabilities

    private var capabilitiesCard: some View {
        SectionCard(String(localized: "Capabilities"), systemImage: "switch.2") {
            VStack(alignment: .leading, spacing: 10) {
                let enabled = Capability.allCases.filter { capabilities.isEnabled($0) }
                if enabled.isEmpty {
                    Text(String(localized: "No capabilities enabled."))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                } else {
                    FlexibleWrap(items: enabled) { cap in
                        Chip(text: cap.label, systemImage: cap.systemImage, color: .brandAccent)
                    }
                }
                let gaps = capabilities.configuredButUnavailable
                if !gaps.isEmpty {
                    Divider()
                    Label {
                        Text(String(
                            format: String(localized: "Configured but unavailable: %@"),
                            gaps.map(\.label).joined(separator: ", ")
                        ))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    } icon: {
                        Image(systemName: "exclamationmark.triangle")
                            .font(.caption)
                            .foregroundStyle(Color.warningAmber)
                    }
                }
            }
        }
    }

    // MARK: - Billing

    private var billingCard: some View {
        SectionCard(String(localized: "Billing"), systemImage: "creditcard") {
            VStack(spacing: 8) {
                if let price = config?.price {
                    DetailRow(
                        label: String(localized: "Price"),
                        value: priceText(price, currency: config?.currency, cycle: config?.billingCycle)
                    )
                }
                if let cycle = config?.billingCycle {
                    DetailRow(label: String(localized: "Cycle"), value: cycle.capitalized)
                }
                if let day = config?.billingStartDay {
                    DetailRow(label: String(localized: "Billing day"), value: "\(day)")
                }
                if let expiry = config?.expiredDate {
                    DetailRow(
                        label: String(localized: "Expires"),
                        value: expiry.formatted(date: .abbreviated, time: .omitted),
                        valueColor: expiry < Date() ? .red : .primary
                    )
                }
                if let limit = config?.trafficLimit {
                    DetailRow(
                        label: String(localized: "Traffic limit"),
                        value: "\(Formatters.formatBytes(limit))\(config?.trafficLimitType.map { " (\($0))" } ?? "")"
                    )
                }
            }
        }
    }

    private func priceText(_ price: Double, currency: String?, cycle: String?) -> String {
        let amount = String(format: "%.2f", price)
        let cur = currency ?? "USD"
        if let cycle { return "\(amount) \(cur) / \(cycle)" }
        return "\(amount) \(cur)"
    }
}
