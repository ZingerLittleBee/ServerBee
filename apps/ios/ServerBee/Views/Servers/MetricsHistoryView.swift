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
            VStack(spacing: 16) {
                timeRangeSelector
                chartSections
            }
            .padding()
        }
        .background(Color(.systemGroupedBackground))
        .task {
            await viewModel.fetchRecords(serverId: serverId, range: selectedRange, apiClient: apiClient)
        }
        .task {
            // History records carry used bytes only; the configured memory and
            // disk totals turn those charts into percentages. Loaded alongside
            // the records so the first render already has the right scale.
            if viewModel.config == nil {
                await viewModel.fetchConfig(serverId: serverId, apiClient: apiClient)
            }
        }
        .onChange(of: selectedRange) { _, newRange in
            Task {
                await viewModel.fetchRecords(serverId: serverId, range: newRange, apiClient: apiClient)
            }
        }
    }

    private var timeRangeSelector: some View {
        HStack(spacing: 8) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(timeRanges, id: \.self) { range in
                        rangePill(range)
                    }
                }
            }
            .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
            // Existing charts stay visible while another range loads.
            if viewModel.isLoading && !viewModel.records.isEmpty {
                ProgressView()
                    .controlSize(.small)
            }
        }
    }

    private func rangePill(_ range: String) -> some View {
        let isSelected = selectedRange == range
        return Button {
            selectedRange = range
        } label: {
            Text(verbatim: range)
                .font(.subheadline.weight(.semibold))
                .monospacedDigit()
                .padding(.horizontal, 16)
                .padding(.vertical, 6)
                .foregroundStyle(isSelected ? Color.white : Color.primary)
                .background(isSelected ? Color.accentColor : Color(.tertiarySystemFill), in: Capsule())
                // Compact pill, but a full 44pt hit target.
                .frame(minHeight: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(isSelected ? [.isSelected] : [])
    }

    /// First load: records are still loading, or they arrived but the server
    /// configuration (which sets the memory / disk scale) is still in flight.
    private var isInitialLoad: Bool {
        if viewModel.records.isEmpty { return viewModel.isLoading }
        return viewModel.config == nil && viewModel.isLoadingConfig
    }

    @ViewBuilder
    private var chartSections: some View {
        if isInitialLoad {
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
            MetricsCharts(
                records: viewModel.records,
                memoryTotal: viewModel.config?.memTotal,
                diskTotal: viewModel.config?.diskTotal
            )
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

// MARK: - Metrics history

/// Renders the standard set of history charts from raw metric records:
/// CPU, Memory, Disk, Network, Disk I/O (when present), Load (when present)
/// and Temperature (when reported). Pure presentation — owns no fetching,
/// scroll, or navigation chrome so it can be embedded in both the standalone
/// history screen and the detail tab.
struct MetricsCharts: View {
    let records: [MetricRecord]
    /// Installed memory / disk capacity from the server configuration. History
    /// records carry used bytes only, so these turn the usage charts into
    /// percentages and complete the "used of total" summaries when known.
    var memoryTotal: Int64?
    var diskTotal: Int64?

    @ScaledMetric(relativeTo: .caption) private var chartHeight: CGFloat = 140

    private var hasDiskIO: Bool {
        records.contains { !$0.diskIoSamples.isEmpty || $0.diskReadPerSec != nil || $0.diskWritePerSec != nil }
    }

    private var hasLoad: Bool {
        records.contains { $0.load1 != nil }
    }

    private var hasTemperature: Bool {
        records.contains { ($0.temperature ?? 0) > 0 }
    }

    var body: some View {
        let data = chartData
        let domain = Self.timeDomain(for: data)
        cpuChart(data, domain: domain)
        memoryChart(data, domain: domain)
        diskChart(data, domain: domain)
        networkChart(data, domain: domain)
        if hasDiskIO { diskIOChart(data, domain: domain) }
        if hasLoad { loadChart(data, domain: domain) }
        if hasTemperature { temperatureChart(data, domain: domain) }
    }
}

private extension MetricsCharts {
    var chartData: [ChartDataPoint] {
        records
            .compactMap { record in
                guard let date = record.date else { return nil }
                return ChartDataPoint(date: date, record: record)
            }
            .sorted { $0.date < $1.date }
    }

    static func timeDomain(for data: [ChartDataPoint]) -> ClosedRange<Date> {
        guard let first = data.first?.date, let last = data.last?.date else {
            let now = Date()
            return now.addingTimeInterval(-3_600)...now
        }
        return first...max(last, first.addingTimeInterval(60))
    }

    static func samples(_ data: [ChartDataPoint], _ value: (MetricRecord) -> Double?) -> [MetricChartSample] {
        data.compactMap { point in
            value(point.record).map { MetricChartSample(date: point.date, value: $0) }
        }
    }

    func cpuChart(_ data: [ChartDataPoint], domain: ClosedRange<Date>) -> some View {
        let title = String(localized: "CPU")
        let samples = Self.samples(data) { $0.cpuUsage }
        return ChartSection(
            title: title,
            subtitle: MetricsSummary.averagePeak(samples) { Formatters.formatPercentage($0) },
            height: chartHeight
        ) {
            MetricHistoryChart(
                valueLabel: title,
                series: [MetricChartSeries(name: title, color: .cpuColor, samples: samples, filled: true)],
                format: .percent,
                timeDomain: domain
            )
        }
    }

    func memoryChart(_ data: [ChartDataPoint], domain: ClosedRange<Date>) -> some View {
        // History records carry `mem_used` bytes but no total; the configured
        // total (when known) turns them into a 0–100 % chart.
        let title = String(localized: "Memory")
        let used = Self.samples(data) { $0.memoryUsed.map(Double.init) }
        let total = MetricsSummary.positive(memoryTotal) ?? MetricsSummary.positive(data.last?.record.memoryTotal)
        return ChartSection(title: title, subtitle: MetricsSummary.usage(used, total: total), height: chartHeight) {
            Self.usageChart(title: title, color: .memoryColor, used: used, total: total, domain: domain)
        }
    }

    func diskChart(_ data: [ChartDataPoint], domain: ClosedRange<Date>) -> some View {
        // Same as memory: records carry `disk_used` bytes only.
        let title = String(localized: "Disk")
        let used = Self.samples(data) { $0.diskUsed.map(Double.init) }
        let total = MetricsSummary.positive(diskTotal) ?? MetricsSummary.positive(data.last?.record.diskTotal)
        return ChartSection(title: title, subtitle: MetricsSummary.usage(used, total: total), height: chartHeight) {
            Self.usageChart(title: title, color: .diskColor, used: used, total: total, domain: domain)
        }
    }

    /// Used-capacity chart: percent of `total` when the capacity is known,
    /// otherwise absolute used bytes on an auto-scaled axis.
    static func usageChart(
        title: String,
        color: Color,
        used: [MetricChartSample],
        total: Double?,
        domain: ClosedRange<Date>
    ) -> MetricHistoryChart {
        let samples = total.map { total in
            used.map { MetricChartSample(date: $0.date, value: $0.value / total * 100) }
        } ?? used
        return MetricHistoryChart(
            valueLabel: title,
            series: [MetricChartSeries(name: title, color: color, samples: samples, filled: true)],
            format: total == nil ? .bytes : .percent,
            timeDomain: domain
        )
    }

    func networkChart(_ data: [ChartDataPoint], domain: ClosedRange<Date>) -> some View {
        let download = MetricChartSeries(
            name: String(localized: "Download"),
            color: .networkColor,
            samples: Self.samples(data) { $0.networkIn.map(Double.init) },
            filled: true
        )
        let upload = MetricChartSeries(
            name: String(localized: "Upload"),
            color: Color(.systemGray),
            samples: Self.samples(data) { $0.networkOut.map(Double.init) }
        )
        let title = String(localized: "Network")
        return ChartSection(
            title: title,
            subtitle: MetricsSummary.network(download: download.samples, upload: upload.samples),
            legend: [download, upload].map { ChartLegendItem(label: $0.name, color: $0.color) },
            height: chartHeight
        ) {
            MetricHistoryChart(valueLabel: title, series: [download, upload], format: .bytesPerSecond, timeDomain: domain)
        }
    }

    func diskIOChart(_ data: [ChartDataPoint], domain: ClosedRange<Date>) -> some View {
        // History records expose disk I/O via the merged disk_io_json sum; the
        // live flat fields are the fallback.
        let read = MetricChartSeries(
            name: String(localized: "Read"),
            color: .diskColor,
            samples: Self.samples(data) { ($0.diskReadMerged ?? $0.diskReadPerSec).map(Double.init) }
        )
        let write = MetricChartSeries(
            name: String(localized: "Write"),
            color: .alertFiring,
            samples: Self.samples(data) { ($0.diskWriteMerged ?? $0.diskWritePerSec).map(Double.init) }
        )
        let title = String(localized: "Disk I/O")
        return ChartSection(
            title: title,
            subtitle: MetricsSummary.diskIO(read: read.samples, write: write.samples),
            legend: [read, write].map { ChartLegendItem(label: $0.name, color: $0.color) },
            height: chartHeight
        ) {
            MetricHistoryChart(valueLabel: title, series: [read, write], format: .bytesPerSecond, timeDomain: domain)
        }
    }

    func loadChart(_ data: [ChartDataPoint], domain: ClosedRange<Date>) -> some View {
        let title = String(localized: "Load Average (1m)")
        let samples = Self.samples(data) { $0.load1 }
        return ChartSection(
            title: title,
            subtitle: MetricsSummary.averagePeak(samples) { $0.formatted(.number.precision(.fractionLength(2))) },
            height: chartHeight
        ) {
            MetricHistoryChart(
                valueLabel: title,
                series: [MetricChartSeries(name: title, color: Color(.systemIndigo), samples: samples, filled: true)],
                format: .decimal,
                timeDomain: domain
            )
        }
    }

    func temperatureChart(_ data: [ChartDataPoint], domain: ClosedRange<Date>) -> some View {
        let title = String(localized: "Temperature")
        let samples = Self.samples(data) { record in
            record.temperature.flatMap { $0 > 0 ? $0 : nil }
        }
        return ChartSection(
            title: title,
            subtitle: samples.last.map { MetricsSummary.celsius($0.value) },
            height: chartHeight
        ) {
            MetricHistoryChart(
                valueLabel: title,
                series: [MetricChartSeries(name: title, color: Color(.systemRed), samples: samples)],
                format: .celsius,
                timeDomain: domain
            )
        }
    }
}

// MARK: - Summaries

/// Card-header summaries computed from the loaded samples.
enum MetricsSummary {
    /// "avg 19.4% · max 29.0%" over the loaded range.
    static func averagePeak(_ samples: [MetricChartSample], format: (Double) -> String) -> String? {
        guard !samples.isEmpty else { return nil }
        let values = samples.map(\.value)
        let average = values.reduce(0, +) / Double(values.count)
        let peak = values.max() ?? average
        return String(localized: "avg \(format(average)) · max \(format(peak))")
    }

    /// Latest used bytes, with the capacity when known ("3.1 GB of 8 GB").
    static func usage(_ samples: [MetricChartSample], total: Double?) -> String? {
        guard let latest = samples.last?.value else { return nil }
        let used = Formatters.formatBytes(Int64(latest))
        guard let total else { return used }
        return String(localized: "\(used) of \(Formatters.formatBytes(Int64(total)))")
    }

    /// Latest download / upload rates ("↓ 4.8 MB/s · ↑ 1.2 MB/s").
    static func network(download: [MetricChartSample], upload: [MetricChartSample]) -> String? {
        guard download.last != nil || upload.last != nil else { return nil }
        return "↓ \(speed(download.last)) · ↑ \(speed(upload.last))"
    }

    /// Latest read / write rates ("read 1.8 MB/s · write 0.7 MB/s").
    static func diskIO(read: [MetricChartSample], write: [MetricChartSample]) -> String? {
        guard read.last != nil || write.last != nil else { return nil }
        return String(localized: "read \(speed(read.last)) · write \(speed(write.last))")
    }

    static func celsius(_ value: Double) -> String {
        Measurement(value: value, unit: UnitTemperature.celsius).formatted(
            .measurement(width: .abbreviated, usage: .asProvided, numberFormatStyle: .number.precision(.fractionLength(0)))
        )
    }

    static func positive(_ value: Int64?) -> Double? {
        guard let value, value > 0 else { return nil }
        return Double(value)
    }

    private static func speed(_ sample: MetricChartSample?) -> String {
        Formatters.formatSpeed(sample.map { Int64($0.value) })
    }
}

#Preview {
    NavigationStack {
        MetricsHistoryView(serverId: "1")
    }
}
