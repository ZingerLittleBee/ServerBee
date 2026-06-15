import SwiftUI

/// Cross-server traffic overview: a fleet-wide daily in/out chart plus a
/// per-server billing-cycle usage breakdown (heaviest users first).
struct FleetTrafficView: View {
    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = FleetTrafficViewModel()

    var body: some View {
        ScrollView {
            content
                .padding(16)
        }
        .background(Color(.systemGroupedBackground))
        .navigationTitle(String(localized: "Traffic"))
        .navigationBarTitleDisplayMode(.inline)
        .refreshable { await viewModel.reload(apiClient: apiClient) }
        .task { await viewModel.loadIfNeeded(apiClient: apiClient) }
    }

    @ViewBuilder
    private var content: some View {
        if viewModel.isLoading && viewModel.servers.isEmpty {
            ProgressView().frame(maxWidth: .infinity).padding(.top, 80)
        } else if let error = viewModel.loadError, viewModel.servers.isEmpty {
            errorState(error)
        } else if viewModel.servers.isEmpty && viewModel.daily.isEmpty {
            emptyState
        } else {
            VStack(spacing: 16) {
                if !viewModel.daily.isEmpty {
                    TrafficDailyChart(daily: viewModel.daily)
                }
                if !viewModel.servers.isEmpty {
                    serversCard
                }
            }
        }
    }

    private var serversCard: some View {
        SectionCard(String(localized: "By server"), systemImage: "server.rack") {
            VStack(spacing: 0) {
                ForEach(viewModel.servers) { server in
                    FleetTrafficRow(server: server)
                    if server.id != viewModel.servers.last?.id {
                        Divider()
                    }
                }
            }
        }
    }

    private var emptyState: some View {
        ContentUnavailableView(
            String(localized: "No traffic data"),
            systemImage: "chart.bar",
            description: Text(String(localized: "No server has a billing cycle configured."))
        )
        .padding(.top, 60)
    }

    private func errorState(_ message: String) -> some View {
        ContentUnavailableView {
            Label(String(localized: "Traffic unavailable"), systemImage: "chart.bar.xaxis")
        } description: {
            Text(message)
        } actions: {
            Button(String(localized: "Retry")) { Task { await viewModel.reload(apiClient: apiClient) } }
        }
        .padding(.top, 60)
    }
}

/// One server's cycle usage: name, totals, an optional usage bar, days left.
private struct FleetTrafficRow: View {
    let server: ServerTrafficOverview

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(server.name).font(.subheadline.weight(.medium))
                Spacer()
                Text(Formatters.formatBytes(server.cycleTotal))
                    .font(.subheadline.monospacedDigit())
            }
            if let percent = server.percentUsed {
                usageBar(percent: percent)
            }
            HStack(spacing: 10) {
                Label(Formatters.formatBytes(server.cycleIn), systemImage: "arrow.down")
                    .foregroundStyle(Color.networkColor)
                Label(Formatters.formatBytes(server.cycleOut), systemImage: "arrow.up")
                    .foregroundStyle(Color.cpuColor)
                Spacer()
                if server.daysRemaining > 0 {
                    Text(String(format: String(localized: "%d days left"), server.daysRemaining))
                        .foregroundStyle(.secondary)
                }
            }
            .font(.caption.monospacedDigit())
        }
        .padding(.vertical, 8)
    }

    @ViewBuilder
    private func usageBar(percent: Double) -> some View {
        let ratio = min(max(percent / 100, 0), 1)
        let color: Color = percent >= 90 ? .serverOffline : (percent >= 75 ? .warningAmber : .serverOnline)
        VStack(spacing: 2) {
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.secondary.opacity(0.15))
                    Capsule().fill(color).frame(width: geo.size.width * ratio)
                }
            }
            .frame(height: 6)
            HStack {
                Text(Formatters.formatPercentage(percent))
                    .font(.caption2.monospacedDigit())
                    .foregroundStyle(color)
                Spacer()
                if let limit = server.trafficLimit {
                    Text(String(format: String(localized: "of %@"), Formatters.formatBytes(limit)))
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }
}
