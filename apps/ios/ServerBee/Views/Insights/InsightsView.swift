import SwiftUI

/// Cross-server "Insights" hub: fleet health, aggregate traffic, cost roll-up,
/// service-monitor status, and operational incidents / maintenance.
struct InsightsView: View {
    @Environment(ServersViewModel.self) private var serversViewModel
    @Environment(\.apiClient) private var apiClient
    @Environment(AuthManager.self) private var authManager
    @State private var viewModel = InsightsViewModel()

    private var isAdmin: Bool { authManager.user?.role.lowercased() == "admin" }
    private var fleet: FleetSummary { FleetSummary.from(serversViewModel.servers) }

    #if DEBUG
    @State private var debugShowIncidents = false
    #endif

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                fleetCard
                trafficCard
                costCard
                fleetTrafficCard
                securityCard
                ipQualityCard
                networkProbesCard
                monitorsCard
                statusCard
            }
            .padding()
        }
        .background(Color(.systemGroupedBackground))
        .navigationTitle(String(localized: "Insights"))
        .refreshable { await viewModel.load(apiClient: apiClient) }
        .task {
            if !viewModel.hasLoaded { await viewModel.load(apiClient: apiClient) }
            #if DEBUG
            if UITestSupport.autoPresent?.hasPrefix("insights-incidents") == true { debugShowIncidents = true }
            #endif
        }
        #if DEBUG
        .navigationDestination(isPresented: $debugShowIncidents) {
            IncidentsView(viewModel: viewModel, isAdmin: isAdmin)
        }
        #endif
    }

    // MARK: - Fleet

    private var fleetCard: some View {
        SectionCard(String(localized: "Fleet"), systemImage: "server.rack") {
            VStack(spacing: 12) {
                HStack(spacing: 12) {
                    stat("\(fleet.total)", String(localized: "Servers"))
                    stat("\(fleet.online)", String(localized: "Online"), color: .serverOnline)
                    stat("\(fleet.offline)", String(localized: "Offline"), color: fleet.offline > 0 ? .serverOffline : .secondary)
                }
                if fleet.avgCpu != nil || fleet.avgMemory != nil {
                    Divider()
                    HStack(spacing: 12) {
                        stat(Formatters.formatPercentage(fleet.avgCpu), String(localized: "Avg CPU"),
                             color: fleet.avgCpu.map { Formatters.cpuColor(for: $0) } ?? .primary)
                        stat(Formatters.formatPercentage(fleet.avgMemory), String(localized: "Avg Memory"),
                             color: fleet.avgMemory.map { Formatters.usageColor(for: $0) } ?? .primary)
                    }
                }
            }
        }
    }

    // MARK: - Traffic

    private var trafficCard: some View {
        SectionCard(String(localized: "Live traffic"), systemImage: "network") {
            VStack(spacing: 10) {
                DetailRow(label: String(localized: "Download"),
                          value: Formatters.formatSpeed(fleet.totalNetworkIn), valueColor: .networkColor)
                DetailRow(label: String(localized: "Upload"),
                          value: Formatters.formatSpeed(fleet.totalNetworkOut), valueColor: .networkColor)
                Divider()
                DetailRow(label: String(localized: "Total received"), value: Formatters.formatBytes(fleet.totalInTransfer))
                DetailRow(label: String(localized: "Total sent"), value: Formatters.formatBytes(fleet.totalOutTransfer))
            }
        }
    }

    // MARK: - Cost

    private var costCard: some View {
        SectionCard(String(localized: "Cost"), systemImage: "creditcard") {
            if let currencies = viewModel.costOverview?.currencies, !currencies.isEmpty {
                VStack(spacing: 12) {
                    ForEach(currencies) { c in
                        VStack(spacing: 4) {
                            HStack {
                                Text(Formatters.formatCurrency(c.monthlyEquivalentTotal, code: c.currency))
                                    .font(.title3.bold())
                                Text(String(localized: "/ mo")).font(.caption).foregroundStyle(.secondary)
                                Spacer()
                                Text(String(format: String(localized: "%d servers"), c.configuredServerCount))
                                    .font(.caption).foregroundStyle(.secondary)
                            }
                            HStack {
                                Text(String(format: String(localized: "%@ / day"),
                                            Formatters.formatCurrency(c.dailyTotal, code: c.currency)))
                                    .font(.caption).foregroundStyle(.secondary)
                                Spacer()
                                Text(String(format: String(localized: "%@ this cycle"),
                                            Formatters.formatCurrency(c.cycleElapsedTotal, code: c.currency)))
                                    .font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            } else {
                Text(String(localized: "No billing configured on any server."))
                    .font(.subheadline).foregroundStyle(.secondary)
            }
        }
    }

    // MARK: - Cross-server overviews

    private var fleetTrafficCard: some View {
        navCard(
            title: String(localized: "Traffic by server"),
            subtitle: String(localized: "Billing-cycle usage & daily history"),
            systemImage: "chart.bar.xaxis", tint: .networkColor
        ) { FleetTrafficView() }
    }

    private var securityCard: some View {
        navCard(
            title: String(localized: "Security"),
            subtitle: String(localized: "Events across all servers"),
            systemImage: "shield.lefthalf.filled", tint: .serverOffline
        ) { FleetSecurityView() }
    }

    private var ipQualityCard: some View {
        navCard(
            title: String(localized: "IP quality"),
            subtitle: String(localized: "Egress IP reputation"),
            systemImage: "shield.checkered", tint: .brandAccent
        ) { FleetIpQualityView() }
    }

    private var networkProbesCard: some View {
        navCard(
            title: String(localized: "Network probes"),
            subtitle: String(localized: "Latency & loss to targets"),
            systemImage: "dot.radiowaves.left.and.right", tint: .cpuColor
        ) { FleetNetworkProbeView() }
    }

    private func navCard<Destination: View>(
        title: String, subtitle: String, systemImage: String, tint: Color,
        @ViewBuilder destination: @escaping () -> Destination
    ) -> some View {
        NavigationLink {
            destination()
        } label: {
            SectionCard {
                HStack(spacing: 12) {
                    Image(systemName: systemImage).foregroundStyle(tint)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(title).font(.subheadline.bold()).foregroundStyle(.primary)
                        Text(subtitle).font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
                }
            }
        }
        .buttonStyle(.plain)
    }

    // MARK: - Monitors

    private var monitorsCard: some View {
        NavigationLink {
            ServiceMonitorsView(monitors: viewModel.monitors, isAdmin: isAdmin)
        } label: {
            SectionCard {
                HStack(spacing: 12) {
                    Image(systemName: "checkmark.shield").foregroundStyle(Color.brandAccent)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(String(localized: "Service monitors")).font(.subheadline.bold()).foregroundStyle(.primary)
                        Text(monitorsSubtitle).font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    if viewModel.monitorsDown > 0 {
                        Chip(text: String(format: String(localized: "%d down"), viewModel.monitorsDown), color: .serverOffline)
                    }
                    Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
                }
            }
        }
        .buttonStyle(.plain)
    }

    private var monitorsSubtitle: String {
        if viewModel.monitors.isEmpty { return String(localized: "None configured") }
        return String(format: String(localized: "%d up · %d total"), viewModel.monitorsUp, viewModel.monitors.count)
    }

    // MARK: - Status

    private var statusCard: some View {
        NavigationLink {
            IncidentsView(viewModel: viewModel, isAdmin: isAdmin)
        } label: {
            SectionCard {
                HStack(spacing: 12) {
                    Image(systemName: viewModel.activeIncidents.isEmpty ? "checkmark.circle" : "exclamationmark.triangle.fill")
                        .foregroundStyle(viewModel.activeIncidents.isEmpty ? Color.serverOnline : Color.warningAmber)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(String(localized: "Status")).font(.subheadline.bold()).foregroundStyle(.primary)
                        Text(statusSubtitle).font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
                }
            }
        }
        .buttonStyle(.plain)
    }

    private var statusSubtitle: String {
        let active = viewModel.activeIncidents.count
        let maint = viewModel.upcomingMaintenances.count
        if active == 0 && maint == 0 { return String(localized: "All systems operational") }
        var parts: [String] = []
        if active > 0 { parts.append(String(format: String(localized: "%d active incident(s)"), active)) }
        if maint > 0 { parts.append(String(format: String(localized: "%d maintenance"), maint)) }
        return parts.joined(separator: " · ")
    }

    // MARK: - Helpers

    private func stat(_ value: String, _ label: String, color: Color = .primary) -> some View {
        VStack(spacing: 2) {
            Text(value).font(.title3.bold()).foregroundStyle(color)
            Text(label).font(.caption).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
    }
}
