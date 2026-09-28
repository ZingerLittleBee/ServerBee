import SwiftUI

/// Server detail "Network" section: history range pills, probe health, latency
/// history per target, per-provider target health, recent anomalies, and an
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
            let palette = NetworkTargetPalette(targets: viewModel.targets, records: viewModel.records)
            VStack(spacing: 16) {
                rangePills
                if let summary = viewModel.summary {
                    NetworkSummaryCard(
                        summary: summary,
                        targetCount: viewModel.targets.isEmpty ? summary.targets.count : viewModel.targets.count
                    )
                }
                NetworkLatencyChart(
                    records: viewModel.records,
                    targets: viewModel.targets,
                    palette: palette,
                    isLoading: viewModel.isLoadingRecords
                )
                NetworkTargetsCard(
                    targets: viewModel.targets,
                    summaries: viewModel.summary?.targets ?? [],
                    palette: palette
                )
                if !viewModel.anomalies.isEmpty {
                    NetworkAnomaliesCard(anomalies: viewModel.anomalies)
                }
                tracerouteButton
            }
        }
    }

    private var tracerouteButton: some View {
        Button {
            showTraceroute = true
        } label: {
            Label(String(localized: "Run Traceroute"), systemImage: "point.topleft.down.to.point.bottomright.curvepath")
                .font(.headline)
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.bordered)
        .buttonBorderShape(.roundedRectangle(radius: 14))
        .controlSize(.large)
        .tint(.accentColor)
    }

    /// Accent-filled capsule range selector, matching the Metrics tab.
    private var rangePills: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(NetworkRange.allCases) { range in
                    let isSelected = viewModel.range == range
                    Button {
                        selectRange(range)
                    } label: {
                        Text(range.label)
                            .font(.subheadline.weight(.semibold))
                            .monospacedDigit()
                            .foregroundStyle(isSelected ? Color.white : Color.primary)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 6)
                            .background(isSelected ? Color.accentColor : Color(.tertiarySystemFill), in: Capsule())
                            // Keep the compact capsule but a 44pt hit target.
                            .frame(minHeight: 44)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(isSelected ? [.isSelected] : [])
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text(String(localized: "Range")))
    }

    private func selectRange(_ range: NetworkRange) {
        guard viewModel.range != range else { return }
        viewModel.range = range
        Task { await viewModel.reloadTimeSeries(serverId: serverId, apiClient: apiClient) }
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

// MARK: - Grouping + palette

/// Targets sharing one probe provider (China Telecom, International, …).
struct NetworkTargetGroup: Identifiable {
    let provider: String
    let targets: [NetworkProbeTarget]

    var id: String { provider }

    /// Group targets by provider, ordered ct/cu/cm/international/custom (ties
    /// broken by the raw provider value so the order is deterministic), with
    /// targets sorted by name inside each group.
    static func groups(_ targets: [NetworkProbeTarget]) -> [NetworkTargetGroup] {
        Dictionary(grouping: targets, by: { $0.provider })
            .map { NetworkTargetGroup(provider: $0.key, targets: $0.value.sorted { $0.name < $1.name }) }
            .sorted { lhs, rhs in
                let l = NetworkProvider.order(for: lhs.provider)
                let r = NetworkProvider.order(for: rhs.provider)
                return l == r ? lhs.provider < rhs.provider : l < r
            }
    }
}

/// Stable per-target colours shared by the latency chart, its legend and the
/// target rows, so one target keeps the same colour across the whole screen.
struct NetworkTargetPalette {
    private static let colors: [Color] = [
        .cpuColor, .memoryColor, .diskColor, .networkColor,
        .pink, .indigo, .orange, .teal, .brown, .cyan
    ]

    private let indexByID: [String: Int]

    /// Assigned targets come first in display order; record-only target ids
    /// (e.g. a target unassigned after it produced history) follow, sorted.
    init(targets: [NetworkProbeTarget], records: [ProbeRecordDto]) {
        var index: [String: Int] = [:]
        let ordered = NetworkTargetGroup.groups(targets).flatMap { $0.targets.map(\.id) }
        let extra = Set(records.map(\.targetId)).subtracting(ordered).sorted()
        for id in ordered + extra where index[id] == nil {
            index[id] = index.count
        }
        indexByID = index
    }

    func color(for targetId: String) -> Color {
        guard let index = indexByID[targetId] else { return .secondary }
        return Self.colors[index % Self.colors.count]
    }

    func order(for targetId: String) -> Int {
        indexByID[targetId] ?? Int.max
    }
}
