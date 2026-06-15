import Charts
import SwiftUI

/// Cross-server network-probe overview: each server's probe health with a 24h
/// latency sparkline, worst latency/loss across targets, and anomaly count.
struct FleetNetworkProbeView: View {
    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = FleetNetworkProbeViewModel()

    var body: some View {
        ScrollView {
            content
                .padding(16)
        }
        .background(Color(.systemGroupedBackground))
        .navigationTitle(String(localized: "Network Probes"))
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
        } else if viewModel.servers.isEmpty {
            emptyState
        } else {
            VStack(spacing: 12) {
                ForEach(viewModel.servers) { server in
                    FleetProbeRow(server: server)
                }
            }
        }
    }

    private var emptyState: some View {
        ContentUnavailableView(
            String(localized: "No probe data"),
            systemImage: "dot.radiowaves.left.and.right",
            description: Text(String(localized: "No server has network probe targets assigned."))
        )
        .padding(.top, 60)
    }

    private func errorState(_ message: String) -> some View {
        ContentUnavailableView {
            Label(String(localized: "Network probes unavailable"), systemImage: "antenna.radiowaves.left.and.right.slash")
        } description: {
            Text(message)
        } actions: {
            Button(String(localized: "Retry")) { Task { await viewModel.reload(apiClient: apiClient) } }
        }
        .padding(.top, 60)
    }
}

/// One server's probe summary card.
private struct FleetProbeRow: View {
    let server: NetworkProbeFleetOverview

    var body: some View {
        SectionCard {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(server.serverName).font(.subheadline.weight(.medium))
                        StatusPill(isOnline: server.online)
                    }
                    Spacer()
                    if server.anomalyCount > 0 {
                        Chip(
                            text: String(localized: "\(server.anomalyCount) anomalies"),
                            systemImage: "exclamationmark.triangle.fill",
                            color: .warningAmber
                        )
                    }
                }
                if server.latencySparkline.contains(where: { $0 != nil }) {
                    Sparkline(values: server.latencySparkline)
                        .frame(height: 32)
                }
                HStack(spacing: 16) {
                    metric(String(localized: "Latency"), NetworkFormat.latency(server.worstLatency))
                    metric(String(localized: "Loss"), NetworkFormat.loss(server.worstLoss))
                    Spacer()
                    Text(String(format: String(localized: "%d targets"), server.targets.count))
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
        }
    }

    private func metric(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(label).font(.caption2).foregroundStyle(.secondary)
            Text(value).font(.subheadline.monospacedDigit())
        }
    }
}

/// Minimal sparkline for a `[Double?]` series (newest last; nil = gap).
private struct Sparkline: View {
    let values: [Double?]

    private struct Point: Identifiable {
        let id: Int
        let value: Double
    }

    private var points: [Point] {
        values.enumerated().compactMap { idx, v in v.map { Point(id: idx, value: $0) } }
    }

    var body: some View {
        Chart(points) { point in
            LineMark(x: .value("i", point.id), y: .value("ms", point.value))
                .foregroundStyle(Color.brandAccent)
                .interpolationMethod(.catmullRom)
        }
        .chartXAxis(.hidden)
        .chartYAxis(.hidden)
        .chartLegend(.hidden)
    }
}
