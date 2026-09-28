import SwiftUI

/// Cross-server "Insights" hub: fleet health, aggregate traffic, cost roll-up,
/// service-monitor status, operational incidents / maintenance, and entry
/// points to the fleet-wide traffic, security, IP-quality and probe screens.
struct InsightsView: View {
    @Environment(ServersViewModel.self) private var serversViewModel
    @Environment(\.apiClient) private var apiClient
    @Environment(AuthManager.self) private var authManager
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @State private var viewModel = InsightsViewModel()
    /// Matches `IconTile`'s scaled size so row separators align with the titles.
    @ScaledMetric(relativeTo: .body) private var rowIconSize: CGFloat = 30

    private var isAdmin: Bool { authManager.user?.role.lowercased() == "admin" }
    private var fleet: FleetSummary { FleetSummary.from(serversViewModel.servers) }

    #if DEBUG
    @State private var debugShowIncidents = false
    @State private var debugShowMonitors = false
    #endif

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                fleetCard
                tileRow { trafficTile } trailing: { costTile }
                tileRow { monitorsTile } trailing: { statusTile }
                destinationsGroup
            }
            .padding(16)
        }
        .background(Color(.systemGroupedBackground))
        .navigationTitle(String(localized: "Insights"))
        .navigationBarTitleDisplayMode(.large)
        .refreshable { await viewModel.load(apiClient: apiClient) }
        .task {
            if !viewModel.hasLoaded { await viewModel.load(apiClient: apiClient) }
            #if DEBUG
            // Both the incident and maintenance sheets live on IncidentsView.
            if let token = UITestSupport.autoPresent,
               token.hasPrefix("insights-incidents") || token == "insights-maintenance-create" {
                debugShowIncidents = true
            }
            if UITestSupport.autoPresent == "insights-monitors" { debugShowMonitors = true }
            #endif
        }
        #if DEBUG
        .navigationDestination(isPresented: $debugShowIncidents) {
            IncidentsView(viewModel: viewModel, isAdmin: isAdmin)
        }
        .navigationDestination(isPresented: $debugShowMonitors) {
            ServiceMonitorsView(monitors: viewModel.monitors, isAdmin: isAdmin)
        }
        #endif
    }
}

// MARK: - Fleet

private extension InsightsView {
    var fleetCard: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .firstTextBaseline) {
                Text(String(localized: "Fleet"))
                    .font(.subheadline.weight(.semibold))
                    .accessibilityAddTraits(.isHeader)
                Spacer(minLength: 8)
                Text(String(localized: "Live"))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            HStack(alignment: .top, spacing: 8) {
                fleetStat(fleet.total, label: String(localized: "Servers"), color: .primary)
                fleetStat(fleet.online, label: String(localized: "Online"), color: .serverOnline)
                fleetStat(fleet.offline, label: String(localized: "Offline"),
                          color: fleet.offline > 0 ? .serverOffline : .secondary)
            }
            if fleet.avgCpu != nil || fleet.avgMemory != nil {
                // Bottom-aligned so the bars line up when only one label wraps.
                adaptiveLayout(spacing: 16, rowAlignment: .bottom) {
                    FleetUsageBar(label: String(localized: "Avg CPU"), percent: fleet.avgCpu, color: .cpuColor,
                                  textColor: .cpuTextColor)
                    FleetUsageBar(label: String(localized: "Avg Memory"), percent: fleet.avgMemory, color: .memoryColor,
                                  textColor: .memoryTextColor)
                }
            }
        }
        .cardSurface()
    }

    func fleetStat(_ value: Int, label: String, color: Color) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            StatValue(value: "\(value)", color: color)
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .minimumScaleFactor(0.7)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(label))
        .accessibilityValue(Text(verbatim: "\(value)"))
    }
}

// MARK: - Traffic & cost

private extension InsightsView {
    var trafficTile: some View {
        tile {
            tileLabel(String(localized: "Live traffic"))
            rateLine(systemImage: "arrow.down", value: Formatters.formatSpeed(fleet.totalNetworkIn),
                     color: .networkTextColor, accessibilityLabel: String(localized: "Download"))
            rateLine(systemImage: "arrow.up", value: Formatters.formatSpeed(fleet.totalNetworkOut),
                     color: .primary, accessibilityLabel: String(localized: "Upload"))
            VStack(alignment: .leading, spacing: 2) {
                totalRow(String(localized: "Total received"), value: Formatters.formatBytes(fleet.totalInTransfer))
                totalRow(String(localized: "Total sent"), value: Formatters.formatBytes(fleet.totalOutTransfer))
            }
            .padding(.top, 4)
        }
    }

    func rateLine(systemImage: String, value: String, color: Color, accessibilityLabel: String) -> some View {
        HStack(spacing: 4) {
            Image(systemName: systemImage)
            Text(verbatim: value)
        }
        .font(.title3.bold())
        .foregroundStyle(color)
        .monospacedDigit()
        .lineLimit(1)
        .minimumScaleFactor(0.6)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(accessibilityLabel))
        .accessibilityValue(Text(verbatim: value))
    }

    /// Label and value side by side; stacks when the half-width tile is too
    /// narrow (large text sizes) instead of truncating the label.
    func totalRow(_ label: String, value: String) -> some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 4) {
                Text(label)
                    .lineLimit(1)
                Spacer(minLength: 4)
                Text(verbatim: value)
                    .monospacedDigit()
                    .lineLimit(1)
            }
            VStack(alignment: .leading, spacing: 0) {
                Text(label)
                Text(verbatim: value)
                    .monospacedDigit()
            }
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .accessibilityElement(children: .combine)
    }

    var costTile: some View {
        tile {
            tileLabel(String(localized: "Cost"))
            if let currencies = viewModel.costOverview?.currencies, !currencies.isEmpty {
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(currencies) { summary in
                        costSummary(summary)
                    }
                }
            } else {
                Text(String(localized: "No billing configured on any server."))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        }
    }

    func costSummary(_ summary: CurrencyCostSummary) -> some View {
        let count = summary.configuredServerCount
        let servers = count == 1 ? String(localized: "\(count) server") : String(localized: "\(count) servers")
        let perDay = String(format: String(localized: "%@ / day"),
                            Formatters.formatCurrency(summary.dailyTotal, code: summary.currency))
        let thisCycle = String(format: String(localized: "%@ burned this cycle"),
                               Formatters.formatCurrency(summary.cycleElapsedTotal, code: summary.currency))
        return VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline, spacing: 3) {
                Text(verbatim: Formatters.formatCurrency(summary.monthlyEquivalentTotal, code: summary.currency))
                    .font(.title2.bold())
                Text(String(localized: "/ mo"))
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.secondary)
            }
            .monospacedDigit()
            .lineLimit(1)
            .minimumScaleFactor(0.6)
            Group {
                // One line when it fits; on narrow screens split at the dot
                // rather than letting the rate wrap mid-phrase.
                ViewThatFits(in: .horizontal) {
                    Text(verbatim: "\(servers) · \(perDay)")
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: servers)
                        Text(verbatim: perDay)
                    }
                }
                Text(verbatim: thisCycle)
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .monospacedDigit()
        }
        .accessibilityElement(children: .combine)
    }
}

// MARK: - Monitors & status

private extension InsightsView {
    var monitorsTile: some View {
        NavigationLink {
            ServiceMonitorsView(monitors: viewModel.monitors, isAdmin: isAdmin)
        } label: {
            linkTile(title: String(localized: "Service monitors"), systemImage: "waveform.path.ecg",
                     subtitle: monitorsSubtitle) {
                if viewModel.monitorsDown > 0 {
                    StatusBadge(text: String(format: String(localized: "%d down"), viewModel.monitorsDown),
                                color: .serverOffline)
                }
            }
        }
        .buttonStyle(.plain)
    }

    var monitorsSubtitle: String {
        if viewModel.monitors.isEmpty { return String(localized: "None configured") }
        return String(format: String(localized: "%d up · %d total"), viewModel.monitorsUp, viewModel.monitors.count)
    }

    var statusTile: some View {
        NavigationLink {
            IncidentsView(viewModel: viewModel, isAdmin: isAdmin)
        } label: {
            linkTile(title: String(localized: "Status"), systemImage: "checkmark.circle", subtitle: statusSubtitle) {
                if let badge = statusBadge {
                    StatusBadge(text: badge.text, color: badge.color)
                }
            }
        }
        .buttonStyle(.plain)
    }

    var statusSubtitle: String {
        let active = viewModel.activeIncidents.count
        let maint = viewModel.upcomingMaintenances.count
        // Incidents are declared by hand, so say exactly that rather than
        // "all systems operational", which reads as a fleet health verdict.
        if active == 0 && maint == 0 { return String(localized: "No active incidents") }
        var parts: [String] = []
        if active > 0 {
            parts.append(active == 1
                ? String(localized: "1 active incident")
                : String(localized: "\(active) active incidents"))
        }
        if maint > 0 {
            parts.append(maint == 1
                ? String(localized: "1 maintenance window")
                : String(localized: "\(maint) maintenance windows"))
        }
        return parts.joined(separator: " · ")
    }

    /// Red for any active critical incident, amber for other active incidents,
    /// accent for an active maintenance window, none when all clear.
    var statusBadge: (text: String, color: Color)? {
        let active = viewModel.activeIncidents
        if active.contains(where: { $0.severity.lowercased() == "critical" }) {
            return (String(localized: "Outage"), .serverOffline)
        }
        if !active.isEmpty { return (String(localized: "Degraded"), .warningAmber) }
        if !viewModel.upcomingMaintenances.isEmpty { return (String(localized: "Maintenance"), .brandAccent) }
        return nil
    }
}

// MARK: - Cross-server destinations

private extension InsightsView {
    var destinationsGroup: some View {
        VStack(spacing: 0) {
            destinationRow(
                title: String(localized: "Traffic by server"),
                subtitle: String(localized: "Billing-cycle usage & daily history"),
                systemImage: "chart.bar.fill", color: .green
            ) { FleetTrafficView() }
            rowDivider
            destinationRow(
                title: String(localized: "Security"),
                subtitle: String(localized: "Events across all servers"),
                systemImage: "shield.lefthalf.filled", color: .red,
                value: viewModel.securityEventCount.flatMap { $0 > 0 ? $0.formatted() : nil }
            ) { FleetSecurityView() }
            rowDivider
            destinationRow(
                title: String(localized: "IP quality"),
                subtitle: String(localized: "Egress IP reputation"),
                systemImage: "checkmark.shield.fill", color: .green
            ) { FleetIpQualityView() }
            rowDivider
            destinationRow(
                title: String(localized: "Network probes"),
                subtitle: String(localized: "Latency & loss to targets"),
                systemImage: "dot.radiowaves.left.and.right", color: .brandAccent
            ) { FleetNetworkProbeView() }
        }
        .background(Color(.secondarySystemGroupedBackground))
        .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
    }

    var rowDivider: some View {
        Divider().padding(.leading, 16 + rowIconSize + 12)
    }

    func destinationRow<Destination: View>(
        title: String, subtitle: String, systemImage: String, color: Color, value: String? = nil,
        @ViewBuilder destination: @escaping () -> Destination
    ) -> some View {
        NavigationLink {
            destination()
        } label: {
            HStack(spacing: 8) {
                IconRowLabel(title: title, systemImage: systemImage, color: color, subtitle: subtitle, value: value)
                Image(systemName: "chevron.right")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.tertiary)
                    .accessibilityHidden(true)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .contentShape(Rectangle())
        }
        .buttonStyle(GroupedRowButtonStyle())
    }
}

// MARK: - Layout helpers

private extension InsightsView {
    /// Side-by-side at regular text sizes, stacked at accessibility sizes.
    func adaptiveLayout<Content: View>(
        spacing: CGFloat, rowAlignment: VerticalAlignment = .top, @ViewBuilder content: () -> Content
    ) -> some View {
        let layout = dynamicTypeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: spacing))
            : AnyLayout(HStackLayout(alignment: rowAlignment, spacing: spacing))
        return layout(content)
    }

    /// Two equal-height tiles in a row (stacked at accessibility sizes).
    func tileRow<Leading: View, Trailing: View>(
        @ViewBuilder _ leading: () -> Leading, @ViewBuilder trailing: () -> Trailing
    ) -> some View {
        adaptiveLayout(spacing: 10) {
            leading()
            trailing()
        }
        .fixedSize(horizontal: false, vertical: true)
    }

    func tile<Content: View>(spacing: CGFloat = 4, @ViewBuilder _ content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: spacing) {
            content()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .cardSurface()
    }

    func tileLabel(_ title: String) -> some View {
        Text(title)
            .font(.footnote.weight(.medium))
            .foregroundStyle(.secondary)
            .accessibilityAddTraits(.isHeader)
    }

    /// A tappable tile: tinted symbol + title, secondary summary line and an
    /// optional status badge.
    func linkTile<Badge: View>(
        title: String, systemImage: String, subtitle: String, @ViewBuilder badge: () -> Badge
    ) -> some View {
        tile(spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Image(systemName: systemImage)
                    .foregroundStyle(Color.accentColor)
                    .accessibilityHidden(true)
                Text(title)
                    .foregroundStyle(.primary)
            }
            .font(.footnote.weight(.semibold))
            Text(subtitle)
                .font(.footnote)
                .foregroundStyle(.secondary)
            badge()
        }
    }
}

// MARK: - Supporting views

/// Fleet-average usage bar: secondary label with a trailing coloured
/// percentage over a thin track. Switches to the warning colour at `warnAt`.
private struct FleetUsageBar: View {
    let label: String
    /// Percentage in `0...100`; `nil` renders a dash and an empty track.
    let percent: Double?
    let color: Color
    /// Colour for the percentage text; a darker variant of `color` for contrast.
    let textColor: Color
    var warnAt: Double = 80
    @ScaledMetric(relativeTo: .footnote) private var barHeight: CGFloat = 6

    private var isHot: Bool { (percent ?? 0) >= warnAt }
    private var tint: Color { isHot ? .warningAmber : color }
    private var fraction: CGFloat { CGFloat(min(max((percent ?? 0) / 100, 0), 1)) }

    private var labelText: some View {
        Text(label)
            .foregroundStyle(.secondary)
    }

    private var percentText: some View {
        Text(verbatim: Formatters.formatPercentage(percent))
            .fontWeight(.semibold)
            .foregroundStyle(percent == nil ? Color.secondary : (isHot ? .warningAmber : textColor))
            .monospacedDigit()
            .lineLimit(1)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            // Label and percentage share a line; they stack when the half-width
            // column is too narrow (large text sizes) instead of truncating.
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .firstTextBaseline, spacing: 4) {
                    labelText.lineLimit(1)
                    Spacer(minLength: 4)
                    percentText
                }
                VStack(alignment: .leading, spacing: 0) {
                    labelText
                    percentText
                }
            }
            .font(.footnote)
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color(.systemGray5))
                    Capsule()
                        .fill(tint)
                        .frame(width: geo.size.width * fraction)
                }
            }
            .frame(height: barHeight)
        }
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(label))
        .accessibilityValue(Text(verbatim: percent == nil
            ? String(localized: "Not available")
            : Formatters.formatPercentage(percent)))
    }
}

/// Grouped-row press feedback for navigation rows on a card surface.
private struct GroupedRowButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(configuration.isPressed ? Color(.systemGray4) : Color.clear)
    }
}
