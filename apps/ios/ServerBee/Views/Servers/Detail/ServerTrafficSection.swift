import SwiftUI

/// Server detail "Traffic" section: billing-cycle usage, daily trend, cost /
/// value insights and a 90-day uptime timeline. All data is member-readable.
struct ServerTrafficSection: View {
    let serverId: String
    let config: ServerConfig?

    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = ServerTrafficViewModel()

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
    }

    @ViewBuilder
    private var content: some View {
        if viewModel.isLoading && viewModel.traffic == nil && viewModel.cost == nil {
            loadingState
        } else if let error = viewModel.loadError {
            errorState(error)
        } else {
            VStack(spacing: 16) {
                if let traffic = viewModel.traffic {
                    TrafficCycleCard(traffic: traffic)
                    if !traffic.daily.isEmpty {
                        TrafficDailyChart(daily: traffic.daily)
                    }
                }
                if let cost = viewModel.cost {
                    CostInsightsCard(cost: cost, config: config)
                }
                UptimeCard(days: viewModel.uptime, windowDays: viewModel.uptimeDays)
            }
        }
    }

    private var loadingState: some View {
        VStack(spacing: 12) {
            ProgressView()
            Text(String(localized: "Loading traffic…"))
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 80)
    }

    private func errorState(_ message: String) -> some View {
        ContentUnavailableView {
            Label(String(localized: "Traffic unavailable"), systemImage: "exclamationmark.triangle")
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
