import Charts
import SwiftUI

/// Displays historical metrics charts for a server using Swift Charts.
/// Includes CPU, Memory, Disk, and Network I/O with a time range selector.
struct MetricsHistoryView: View {
    let serverId: String

    @Environment(AuthManager.self) private var authManager
    @State private var viewModel = ServerDetailViewModel()
    @State private var selectedRange = "1h"
    @State private var apiClient: APIClient?

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
        .navigationTitle(String(localized: "Metrics History"))
        .navigationBarTitleDisplayMode(.inline)
        .task {
            let client = APIClient(authManager: authManager)
            apiClient = client
            await viewModel.fetchRecords(serverId: serverId, range: selectedRange, apiClient: client)
        }
        .onChange(of: selectedRange) { _, newRange in
            guard let apiClient else { return }
            Task {
                await viewModel.fetchRecords(serverId: serverId, range: newRange, apiClient: apiClient)
            }
        }
    }

    // MARK: - Time Range Selector

    private var timeRangeSelector: some View {
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
            }
            Spacer()
        }
    }

    // MARK: - Chart Sections

    @ViewBuilder
    private var chartSections: some View {
        if viewModel.isLoading {
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
            let data = chartData
            cpuChart(data: data)
            memoryChart(data: data)
            diskChart(data: data)
            networkChart(data: data)
        }
    }

    // MARK: - CPU Chart

    private func cpuChart(data: [ChartDataPoint]) -> some View {
        ChartSection(title: String(localized: "CPU Usage")) {
            Chart {
                ForEach(data, id: \.date) { point in
                    if let cpu = point.record.cpuUsage {
                        LineMark(
                            x: .value("Time", point.date),
                            y: .value("CPU %", cpu)
                        )
                        .foregroundStyle(Color.cpuColor)
                        .interpolationMethod(.catmullRom)
                    }
                }
            }
            .percentageYAxis()
            .timeXAxis()
        }
    }

    // MARK: - Memory Chart

    private func memoryChart(data: [ChartDataPoint]) -> some View {
        ChartSection(title: String(localized: "Memory Usage")) {
            Chart {
                ForEach(data, id: \.date) { point in
                    if let percent = point.record.memoryPercent {
                        AreaMark(
                            x: .value("Time", point.date),
                            y: .value("Memory %", percent)
                        )
                        .foregroundStyle(
                            .linearGradient(
                                colors: [Color.memoryColor.opacity(0.3), Color.memoryColor.opacity(0.05)],
                                startPoint: .top,
                                endPoint: .bottom
                            )
                        )
                        .interpolationMethod(.catmullRom)

                        LineMark(
                            x: .value("Time", point.date),
                            y: .value("Memory %", percent)
                        )
                        .foregroundStyle(Color.memoryColor)
                        .interpolationMethod(.catmullRom)
                    }
                }
            }
            .percentageYAxis()
            .timeXAxis()
        }
    }

    // MARK: - Disk Chart

    private func diskChart(data: [ChartDataPoint]) -> some View {
        ChartSection(title: String(localized: "Disk Usage")) {
            Chart {
                ForEach(data, id: \.date) { point in
                    if let percent = point.record.diskPercent {
                        AreaMark(
                            x: .value("Time", point.date),
                            y: .value("Disk %", percent)
                        )
                        .foregroundStyle(
                            .linearGradient(
                                colors: [Color.diskColor.opacity(0.3), Color.diskColor.opacity(0.05)],
                                startPoint: .top,
                                endPoint: .bottom
                            )
                        )
                        .interpolationMethod(.catmullRom)

                        LineMark(
                            x: .value("Time", point.date),
                            y: .value("Disk %", percent)
                        )
                        .foregroundStyle(Color.diskColor)
                        .interpolationMethod(.catmullRom)
                    }
                }
            }
            .percentageYAxis()
            .timeXAxis()
        }
    }

    // MARK: - Network Chart

    private func networkChart(data: [ChartDataPoint]) -> some View {
        ChartSection(title: String(localized: "Network I/O")) {
            Chart {
                ForEach(data, id: \.date) { point in
                    if let netIn = point.record.networkIn {
                        LineMark(
                            x: .value("Time", point.date),
                            y: .value("Bytes", netIn),
                            series: .value("Direction", "In")
                        )
                        .foregroundStyle(Color.networkColor)
                        .interpolationMethod(.catmullRom)
                    }
                    if let netOut = point.record.networkOut {
                        LineMark(
                            x: .value("Time", point.date),
                            y: .value("Bytes", netOut),
                            series: .value("Direction", "Out")
                        )
                        .foregroundStyle(Color.cpuColor)
                        .interpolationMethod(.catmullRom)
                    }
                }
            }
            .chartForegroundStyleScale([
                "In": Color.networkColor,
                "Out": Color.cpuColor,
            ])
            .timeXAxis()
        }
    }

    // MARK: - Chart Data Helper

    /// Pre-processes records into (date, record) pairs, filtering out those with unparsable timestamps.
    private var chartData: [ChartDataPoint] {
        viewModel.records.compactMap { record in
            guard let date = record.date else { return nil }
            return ChartDataPoint(date: date, record: record)
        }
    }
}

// MARK: - Chart Data Point

private struct ChartDataPoint {
    let date: Date
    let record: MetricRecord
}

// MARK: - Chart Section

/// A reusable container for a chart with a title.
private struct ChartSection<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.headline)

            content
                .frame(height: 200)
        }
        .padding()
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
    }
}

// MARK: - Shared Chart Axis Modifiers

private struct PercentageYAxisModifier: ViewModifier {
    func body(content: Content) -> some View {
        content
            .chartYScale(domain: 0...100)
            .chartYAxis {
                AxisMarks(values: [0, 25, 50, 75, 100]) { value in
                    AxisGridLine()
                    AxisValueLabel {
                        if let v = value.as(Int.self) {
                            Text("\(v)%")
                        }
                    }
                }
            }
    }
}

private struct TimeXAxisModifier: ViewModifier {
    func body(content: Content) -> some View {
        content
            .chartXAxis {
                AxisMarks { value in
                    AxisGridLine()
                    AxisValueLabel {
                        if let date = value.as(Date.self) {
                            Text(Formatters.formatChartTime(date))
                        }
                    }
                }
            }
    }
}

private extension View {
    func percentageYAxis() -> some View {
        modifier(PercentageYAxisModifier())
    }

    func timeXAxis() -> some View {
        modifier(TimeXAxisModifier())
    }
}

#Preview {
    NavigationStack {
        MetricsHistoryView(serverId: "1")
    }
    .environment(AuthManager())
}
