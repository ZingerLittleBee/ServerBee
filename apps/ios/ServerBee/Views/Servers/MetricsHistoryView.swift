import SwiftUI

/// Range selector + history charts for a server. Reusable content (no
/// navigation chrome) so it can back both the standalone screen and the
/// detail "Metrics" tab. Consumes `APIClient` from the environment.
struct MetricsContentView: View {
    let serverId: String

    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = ServerDetailViewModel()
    @State private var selectedRange = "1h"

    private let timeRanges = ["1h", "6h", "24h", "7d"]

    var body: some View {
        ScrollView {
            VStack(spacing: 20) {
                timeRangeSelector
                chartSections
            }
            .padding()
        }
        .background(Color(.systemGroupedBackground))
        .task {
            await viewModel.fetchRecords(serverId: serverId, range: selectedRange, apiClient: apiClient)
        }
        .onChange(of: selectedRange) { _, newRange in
            Task {
                await viewModel.fetchRecords(serverId: serverId, range: newRange, apiClient: apiClient)
            }
        }
    }

    private var timeRangeSelector: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(timeRanges, id: \.self) { range in
                    Button {
                        selectedRange = range
                    } label: {
                        Text(range)
                            .font(.subheadline.bold())
                            .padding(.horizontal, 14)
                            .padding(.vertical, 8)
                            .background(selectedRange == range ? Color.accentColor : Color(.systemGray5))
                            .foregroundStyle(selectedRange == range ? .white : .primary)
                            .clipShape(Capsule())
                    }
                    .accessibilityAddTraits(selectedRange == range ? [.isSelected] : [])
                }
            }
            .padding(.horizontal, 2)
        }
    }

    @ViewBuilder
    private var chartSections: some View {
        if viewModel.isLoading && viewModel.records.isEmpty {
            VStack(spacing: 16) {
                ProgressView()
                Text(String(localized: "Loading metrics..."))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity)
            .padding(.top, 40)
        } else if viewModel.records.isEmpty {
            ContentUnavailableView {
                Label(String(localized: "No Data"), systemImage: "chart.line.downtrend.xyaxis")
            } description: {
                Text(String(localized: "No metric records found for this time range."))
            }
        } else {
            MetricsCharts(records: viewModel.records)
        }
    }
}

/// Standalone history screen (still reachable as a pushed destination).
struct MetricsHistoryView: View {
    let serverId: String

    var body: some View {
        MetricsContentView(serverId: serverId)
            .navigationTitle(String(localized: "Metrics History"))
            .navigationBarTitleDisplayMode(.inline)
    }
}

#Preview {
    NavigationStack {
        MetricsHistoryView(serverId: "1")
    }
}
