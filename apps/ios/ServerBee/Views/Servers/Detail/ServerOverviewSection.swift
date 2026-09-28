import SwiftUI

/// The "Overview" tab of the server detail screen: live metric tiles, runtime
/// counters, system info, billing and agent capabilities — composed from the
/// live WS status (metrics) and the REST config (static metadata). Admins also
/// get the advanced-tools entry points and the agent lifecycle card.
///
/// The status chips, section picker and edit action live in the detail shell
/// (`ServerDetailView`), which is shared by every section.
struct ServerOverviewSection: View {
    let serverId: String
    let live: ServerStatus?
    let config: ServerConfig?
    let capabilities: CapabilitySet
    let isAdmin: Bool
    /// Re-fetch the server's REST config after an edit / enrollment change.
    var onReloadConfig: () -> Void = {}

    @Environment(\.dismiss) private var dismiss
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.privacyMode) private var privacyMode

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                if isPending {
                    pendingBanner
                }
                if let live, hasAnyMetric {
                    metricsGrid(live)
                    // Live counters; an offline server's stale values would read as current.
                    let runtime = isOnline ? runtimeRows(live) : []
                    if !runtime.isEmpty {
                        OverviewInfoGroup(title: String(localized: "Runtime")) {
                            OverviewInfoCard(rows: runtime)
                        }
                    }
                }
                let system = systemRows
                if !system.isEmpty {
                    OverviewInfoGroup(title: String(localized: "System info")) {
                        OverviewInfoCard(rows: system)
                    }
                }
                if hasBilling {
                    OverviewInfoGroup(title: String(localized: "Billing")) {
                        billingCard
                    }
                }
                OverviewInfoGroup(title: String(localized: "Agent capabilities")) {
                    capabilitiesCard
                }
                if isAdmin {
                    AdvancedToolsCard(capabilities: capabilities)
                    ServerLifecycleCard(
                        serverId: serverId,
                        config: config,
                        capabilities: capabilities,
                        isOnline: isOnline,
                        isPending: isPending,
                        onConfigChanged: onReloadConfig,
                        onDeleted: { dismiss() }
                    )
                }
            }
            .padding(16)
        }
        .background(Color(.systemGroupedBackground))
    }

    // MARK: - Derived

    private var isOnline: Bool { live?.isOnline ?? false }

    /// A live reading, or `nil` while offline so tiles show "—", not stale values.
    private func current<T>(_ value: T?) -> T? { isOnline ? value : nil }
    private var isPending: Bool {
        if let config { return !config.isEnrolled }
        if let live { return !live.hasAgentAuthority }
        return false
    }

    private var hasAnyMetric: Bool {
        guard let s = live else { return false }
        return s.cpuUsage != nil || s.memoryUsed != nil || s.diskUsed != nil || s.load1 != nil
    }

    /// Matches the web detail page: an expiry date alone is billing info too.
    private var hasBilling: Bool {
        config?.price != nil || config?.billingCycle != nil || config?.trafficLimit != nil || config?.expiredDate != nil
    }

    // MARK: - Pending enrollment

    private var pendingBanner: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "person.badge.clock")
                .foregroundStyle(Color.warningAmber)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(String(localized: "Pending enrollment"))
                    .font(.subheadline.weight(.semibold))
                Text(String(localized: "This server is waiting for its agent to connect with an enrollment code."))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.warningAmber.opacity(0.12), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .accessibilityElement(children: .combine)
    }

    // MARK: - Metrics grid

    /// 2x2 tiles; a single column at accessibility text sizes, where half-width
    /// tiles would shrink or truncate the readouts.
    @ViewBuilder
    private func metricsGrid(_ s: ServerStatus) -> some View {
        if dynamicTypeSize.isAccessibilitySize {
            VStack(spacing: 10) {
                cpuTile(s)
                memoryTile(s)
                diskTile(s)
                networkTile(s)
            }
        } else {
            Grid(horizontalSpacing: 10, verticalSpacing: 10) {
                GridRow {
                    cpuTile(s)
                    memoryTile(s)
                }
                GridRow {
                    diskTile(s)
                    networkTile(s)
                }
            }
        }
    }
}

// MARK: - Metric tiles

private extension ServerOverviewSection {
    func cpuTile(_ s: ServerStatus) -> some View {
        let usage = current(s.cpuUsage)
        return MetricCardView(
            String(localized: "CPU"),
            accessibilityValue: [usage.map { Formatters.formatPercentage($0) }, s.cpuName]
                .compactMap { $0 }.joined(separator: ", ")
        ) {
            MetricTileReadout(
                value: usage.map { String(format: "%.1f", $0) } ?? "—",
                unit: usage == nil ? nil : "%",
                color: usage == nil ? .secondary : Self.usageTint(usage, base: .cpuColor)
            )
            if let cpu = usage {
                UsageBar(value: cpu / 100, height: 6, tint: Self.usageTint(cpu, base: .cpuColor))
            }
            if let name = s.cpuName {
                Self.tileCaption(name)
            }
        }
    }

    func memoryTile(_ s: ServerStatus) -> some View {
        let used = current(s.memoryUsed)
        let percent = current(s.memoryPercent)
        let parts = Self.sharedUnitParts(used: used, total: s.memoryTotal)
        return MetricCardView(
            String(localized: "Memory"),
            accessibilityValue: [
                percent.map { Formatters.formatPercentage($0) },
                Formatters.formatBytesRatio(used: used, total: s.memoryTotal)
            ].compactMap { $0 }.joined(separator: ", ")
        ) {
            MetricTileReadout(
                value: parts?.used ?? "—",
                unit: parts.map { "/ \($0.total)" },
                color: parts == nil ? .secondary : Self.usageTint(percent, base: .memoryColor)
            )
            if let percent {
                UsageBar(value: percent / 100, height: 6, tint: Self.usageTint(percent, base: .memoryColor))
                Self.tileCaption(Formatters.formatPercentage(percent))
            } else if let total = s.memoryTotal, total > 0 {
                Self.tileCaption(String(localized: "\(Formatters.formatBytes(total)) total"))
            }
        }
    }

    func diskTile(_ s: ServerStatus) -> some View {
        let used = current(s.diskUsed)
        let percent = current(s.diskPercent)
        let parts = Self.sharedUnitParts(used: used, total: s.diskTotal)
        return MetricCardView(
            String(localized: "Disk"),
            accessibilityValue: [
                percent.map { Formatters.formatPercentage($0) },
                Formatters.formatBytesRatio(used: used, total: s.diskTotal)
            ].compactMap { $0 }.joined(separator: ", ")
        ) {
            MetricTileReadout(
                value: percent.map { String(format: "%.1f", $0) } ?? "—",
                unit: percent == nil ? nil : "%",
                color: percent == nil ? .secondary : Self.usageTint(percent, base: .diskColor)
            )
            if let percent {
                UsageBar(value: percent / 100, height: 6, tint: Self.usageTint(percent, base: .diskColor))
            }
            if let parts {
                Self.tileCaption("\(parts.used) / \(parts.total)")
            } else if let total = s.diskTotal, total > 0 {
                Self.tileCaption(String(localized: "\(Formatters.formatBytes(total)) total"))
            }
        }
    }

    func networkTile(_ s: ServerStatus) -> some View {
        let down = current(s.networkIn).map { Formatters.formatSpeed($0) } ?? "—"
        let up = current(s.networkOut).map { Formatters.formatSpeed($0) } ?? "—"
        return MetricCardView(String(localized: "Network"), accessibilityValue: "↓ \(down), ↑ \(up)") {
            MetricTileReadout(
                value: down,
                color: isOnline ? .networkColor : .secondary,
                font: .title3.weight(.bold),
                unitFont: .subheadline.weight(.bold),
                systemImage: "arrow.down"
            )
            MetricTileReadout(
                value: up,
                color: isOnline ? .primary : .secondary,
                font: .title3.weight(.bold),
                unitFont: .subheadline.weight(.bold),
                systemImage: "arrow.up"
            )
        }
    }

    static func tileCaption(_ text: String) -> some View {
        Text(text)
            .font(.caption2)
            .foregroundStyle(.tertiary)
            .monospacedDigit()
            .lineLimit(1)
    }

    /// Metric identity colour, escalating to warning / critical at high usage.
    static func usageTint(_ percent: Double?, base: Color) -> Color {
        guard let percent else { return base }
        if percent >= 90 { return .serverOffline }
        if percent >= 80 { return .warningAmber }
        return base
    }

    /// Formats `used` in the unit chosen for `total`, so the pair reads as
    /// `3.1` + `8 GB` (rendered "3.1 / 8 GB") instead of repeating the unit.
    static func sharedUnitParts(used: Int64?, total: Int64?) -> (used: String, total: String)? {
        guard let used, let total, total > 0 else { return nil }
        let unit: ByteCountFormatter.Units
        let divisor: Int64
        switch total {
        case (1 << 40)...: (unit, divisor) = (.useTB, 1 << 40)
        case (1 << 30)...: (unit, divisor) = (.useGB, 1 << 30)
        case (1 << 20)...: (unit, divisor) = (.useMB, 1 << 20)
        default: (unit, divisor) = (.useKB, 1 << 10)
        }
        let formatter = ByteCountFormatter()
        formatter.countStyle = .binary
        formatter.allowedUnits = unit
        formatter.allowsNonnumericFormatting = false
        // One decimal keeps the big readout calm ("26.2 / 32 GB", not "26.24").
        let value = Double(used) / Double(divisor)
        let usedText = String(format: value >= 100 ? "%.0f" : "%.1f", value)
        return (usedText, formatter.string(fromByteCount: total))
    }
}

// MARK: - Runtime + system rows

private extension ServerOverviewSection {
    func runtimeRows(_ s: ServerStatus) -> [OverviewInfoRow] {
        var rows: [OverviewInfoRow] = []
        if let load1 = s.load1 {
            let loads = [load1, s.load5, s.load15].compactMap { $0 }.map { String(format: "%.2f", $0) }
            rows.append(OverviewInfoRow(label: String(localized: "Load"), value: loads.joined(separator: " · ")))
        }
        if let processes = s.processCount {
            rows.append(OverviewInfoRow(label: String(localized: "Processes"), value: "\(processes)"))
        }
        if s.tcpCount != nil || s.udpCount != nil {
            rows.append(OverviewInfoRow(
                label: String(localized: "TCP / UDP"),
                value: "\(s.tcpCount.map(String.init) ?? "—") / \(s.udpCount.map(String.init) ?? "—")"
            ))
        }
        if let swapTotal = s.swapTotal, swapTotal > 0 {
            let ratio = Formatters.formatBytesRatio(used: s.swapUsed, total: s.swapTotal)
            rows.append(OverviewInfoRow(
                label: String(localized: "Swap"),
                value: [Formatters.formatPercentage(s.swapPercent), ratio].compactMap { $0 }.joined(separator: " · "),
                valueColor: s.swapPercent.map { Self.usageTint($0, base: .secondary) }
            ))
        }
        if s.diskReadPerSec != nil || s.diskWritePerSec != nil {
            rows.append(OverviewInfoRow(
                label: String(localized: "Disk I/O"),
                value: "R \(Formatters.formatSpeed(s.diskReadPerSec)) · W \(Formatters.formatSpeed(s.diskWritePerSec))"
            ))
        }
        if let inT = s.netInTransfer, let outT = s.netOutTransfer {
            rows.append(OverviewInfoRow(
                label: String(localized: "Transfer"),
                value: "↓ \(Formatters.formatBytes(inT)) · ↑ \(Formatters.formatBytes(outT))"
            ))
        }
        return rows
    }

    var systemRows: [OverviewInfoRow] {
        let candidates: [(String, String?)] = [
            (String(localized: "IPv4"), (config?.ipv4 ?? live?.ipv4)?.maskingIPs(privacyMode)),
            (String(localized: "IPv6"), (config?.ipv6 ?? live?.ipv6)?.maskingIPs(privacyMode)),
            (String(localized: "Location"), locationText),
            (String(localized: "OS"), config?.os ?? live?.os),
            (String(localized: "Kernel"), config?.kernelVersion),
            (String(localized: "Architecture"), config?.cpuArch),
            (String(localized: "CPU"), config?.cpuName ?? live?.cpuName),
            (String(localized: "Cores"), (config?.cpuCores ?? live?.cpuCores).map { "\($0)" }),
            (String(localized: "Virtualization"), config?.virtualization),
            (String(localized: "Agent"), config?.agentVersion),
            (String(localized: "Last boot"), lastBootText)
        ]
        return candidates.compactMap { label, value in
            guard let value, !value.isEmpty else { return nil }
            return OverviewInfoRow(label: label, value: value)
        }
    }

    var locationText: String? {
        let region = config?.region ?? live?.region
        let country = config?.countryCode ?? live?.country
        let place: String? = switch (region, country) {
        case let (r?, c?): "\(r), \(c)"
        case let (r?, nil): r
        case let (nil, c?): c
        default: nil
        }
        guard let place else { return nil }
        if let flag = CountryFlag.emoji(for: country) { return "\(flag) \(place)" }
        return place
    }

    /// Boot time derived from the live uptime counter (only while online, when
    /// the counter is current).
    var lastBootText: String? {
        guard isOnline, let uptime = live?.uptime, uptime > 0 else { return nil }
        return Date(timeIntervalSinceNow: -TimeInterval(uptime)).formatted(date: .abbreviated, time: .shortened)
    }
}

// MARK: - Billing + capabilities

private extension ServerOverviewSection {
    var billingCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let price = config?.price {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    HStack(alignment: .firstTextBaseline, spacing: 4) {
                        Text(Formatters.formatCurrency(price, code: config?.currency ?? "USD"))
                            .font(.title2.weight(.bold))
                            .monospacedDigit()
                        if let cycle = config?.billingCycle {
                            Text(Self.perCycleText(cycle))
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.secondary)
                        }
                    }
                    // An explicit label replaces a combined one, so the amount
                    // is carried as the value instead.
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(Text(String(localized: "Price")))
                    .accessibilityValue(Text(priceAccessibilityValue(price)))
                    Spacer(minLength: 8)
                    expiryText
                }
            } else {
                expiryText
            }
            ForEach(billingRows) { row in
                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    Text(row.label)
                    Spacer(minLength: 12)
                    Text(row.value)
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                        .multilineTextAlignment(.trailing)
                }
                .font(.footnote)
                .accessibilityElement(children: .combine)
            }
        }
        .cardSurface()
    }

    func priceAccessibilityValue(_ price: Double) -> String {
        let amount = Formatters.formatCurrency(price, code: config?.currency ?? "USD")
        guard let cycle = config?.billingCycle else { return amount }
        return "\(amount) \(Self.perCycleText(cycle))"
    }

    @ViewBuilder
    var expiryText: some View {
        if let expiry = config?.expiredDate {
            let date = expiry.formatted(date: .abbreviated, time: .omitted)
            let isPast = expiry < Date()
            Text(isPast ? String(localized: "Expired \(date)") : String(localized: "Expires \(date)"))
                .font(.footnote.weight(isPast ? .semibold : .regular))
                .foregroundStyle(isPast ? Color.serverOffline : .secondary)
        }
    }

    var billingRows: [OverviewInfoRow] {
        var rows: [OverviewInfoRow] = []
        // With a price, the cycle already reads as "/ month" next to it.
        if config?.price == nil, let cycle = config?.billingCycle {
            rows.append(OverviewInfoRow(label: String(localized: "Cycle"), value: Self.cycleName(cycle)))
        }
        if let day = config?.billingStartDay {
            rows.append(OverviewInfoRow(label: String(localized: "Billing day"), value: "\(day)"))
        }
        if let limit = config?.trafficLimit {
            rows.append(OverviewInfoRow(
                label: String(localized: "Traffic limit"),
                value: "\(Formatters.formatBytes(limit))\(config?.trafficLimitType.map { " (\($0))" } ?? "")"
            ))
        }
        return rows
    }

    static func cycleName(_ cycle: String) -> String {
        switch cycle {
        case "monthly": String(localized: "Monthly")
        case "quarterly": String(localized: "Quarterly")
        case "yearly": String(localized: "Yearly")
        default: cycle.capitalized
        }
    }

    static func perCycleText(_ cycle: String) -> String {
        switch cycle {
        case "monthly": String(localized: "/ month")
        case "quarterly": String(localized: "/ quarter")
        case "yearly": String(localized: "/ year")
        default: "/ \(cycle)"
        }
    }

    var capabilitiesCard: some View {
        let enabled = Capability.allCases.filter { capabilities.isEnabled($0) }
        let disabled = Capability.allCases.filter { !capabilities.isEnabled($0) }
        let gaps = capabilities.configuredButUnavailable
        // With no mask at all (config not loaded / fetch failed) nothing is
        // known, so don't list every capability as "off".
        let isKnown = capabilities.configured != nil || capabilities.agentLocal != nil || capabilities.effective != nil
        return VStack(alignment: .leading, spacing: 10) {
            if enabled.isEmpty {
                Text(String(localized: "No capabilities enabled."))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            if isKnown {
                FlexibleWrap(items: enabled + disabled) { cap in
                    if enabled.contains(cap) {
                        Chip(text: cap.label, color: .accentColor)
                    } else {
                        Chip(text: String(localized: "\(cap.label) · off"), color: .secondary)
                    }
                }
            }
            if !gaps.isEmpty {
                Label {
                    Text(String(
                        format: String(localized: "Configured but unavailable: %@"),
                        gaps.map(\.label).joined(separator: ", ")
                    ))
                } icon: {
                    Image(systemName: "exclamationmark.triangle")
                        .foregroundStyle(Color.warningAmber)
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            Text(String(localized: "Set on the agent host. Read-only here."))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .cardSurface()
    }
}
