import SwiftUI

/// Server detail "Network" section: probe summary, per-provider target health,
/// latency/loss history over a selectable range, recent anomalies, and an
/// interactive traceroute entry. Gated on the server's ping capabilities by the
/// parent detail view.
struct ServerNetworkSection: View {
    let serverId: String
    let isAdmin: Bool

    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = ServerNetworkViewModel()
    @State private var showTraceroute = false

    var body: some View {
        ScrollView {
            content
                .padding(16)
        }
        .background(Color(.systemGroupedBackground))
        .scrollIndicators(.hidden)
        .refreshable {
            await viewModel.reload(serverId: serverId, apiClient: apiClient)
        }
        .task {
            await viewModel.loadIfNeeded(serverId: serverId, apiClient: apiClient)
        }
        .sheet(isPresented: $showTraceroute) {
            TracerouteView(serverId: serverId, serverOnline: viewModel.summary?.online ?? true)
        }
    }

    @ViewBuilder
    private var content: some View {
        if viewModel.isLoading && viewModel.summary == nil {
            loadingState
        } else if let error = viewModel.loadError {
            errorState(error)
        } else {
            VStack(spacing: 16) {
                if let summary = viewModel.summary {
                    NetworkSummaryCard(summary: summary)
                }
                tracerouteButton
                rangePicker
                NetworkLatencyChart(records: viewModel.records, targets: viewModel.targets)
                NetworkTargetsCard(targets: viewModel.targets, summaries: viewModel.summary?.targets ?? [])
                if !viewModel.anomalies.isEmpty {
                    NetworkAnomaliesCard(anomalies: viewModel.anomalies)
                }
            }
        }
    }

    private var tracerouteButton: some View {
        Button {
            showTraceroute = true
        } label: {
            Label(String(localized: "Run Traceroute"), systemImage: "point.topleft.down.to.point.bottomright.curvepath")
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
    }

    private var rangePicker: some View {
        Picker(String(localized: "Range"), selection: Binding(
            get: { viewModel.range },
            set: { newValue in
                viewModel.range = newValue
                Task { await viewModel.reloadTimeSeries(serverId: serverId, apiClient: apiClient) }
            }
        )) {
            ForEach(NetworkRange.allCases) { r in
                Text(r.label).tag(r)
            }
        }
        .pickerStyle(.segmented)
    }

    private var loadingState: some View {
        VStack(spacing: 12) {
            ProgressView()
            Text(String(localized: "Loading network…"))
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 80)
    }

    private func errorState(_ message: String) -> some View {
        ContentUnavailableView {
            Label(String(localized: "Network unavailable"), systemImage: "wifi.exclamationmark")
        } description: {
            Text(message)
        } actions: {
            Button(String(localized: "Retry")) {
                Task { await viewModel.reload(serverId: serverId, apiClient: apiClient) }
            }
        }
        .padding(.top, 60)
    }
}
