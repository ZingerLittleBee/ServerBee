import Charts
import SwiftUI

/// A reusable container for a chart with a title.
struct ChartSection<Content: View>: View {
    let title: String
    var subtitle: String?
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                Text(title)
                    .font(.headline)
                if let subtitle {
                    Spacer()
                    Text(subtitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            content
                .frame(height: 200)
        }
        .padding()
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
    }
}

/// (date, record) pair with a guaranteed-parsable timestamp.
struct ChartDataPoint {
    let date: Date
    let record: MetricRecord
}

/// Renders the standard set of history charts from raw metric records:
/// CPU %, Memory %, Disk %, Load, Network I/O, and (when present) Disk I/O.
/// Pure presentation — owns no fetching, scroll, or navigation chrome so it
/// can be embedded in both the standalone history screen and the detail tab.
struct MetricsCharts: View {
    let records: [MetricRecord]

    private var data: [ChartDataPoint] {
        records.compactMap { record in
            guard let date = record.date else { return nil }
            return ChartDataPoint(date: date, record: record)
        }
    }

    private var hasDiskIO: Bool {
        records.contains { ($0.diskReadPerSec != nil) || ($0.diskWritePerSec != nil) }
    }

    private var hasLoad: Bool {
        records.contains { $0.load1 != nil }
    }

    var body: some View {
        let data = data
        cpuChart(data: data)
        memoryChart(data: data)
        diskChart(data: data)
        if hasLoad { loadChart(data: data) }
        networkChart(data: data)
        if hasDiskIO { diskIOChart(data: data) }
    }

    private func cpuChart(data: [ChartDataPoint]) -> some View {
        ChartSection(title: String(localized: "CPU Usage")) {
            Chart(data, id: \.date) { point in
                if let cpu = point.record.cpuUsage {
                    LineMark(x: .value("Time", point.date), y: .value("CPU %", cpu))
                        .foregroundStyle(Color.cpuColor)
                        .interpolationMethod(.catmullRom)
                }
            }
            .percentageYAxis()
            .timeXAxis()
        }
    }

    private func memoryChart(data: [ChartDataPoint]) -> some View {
        ChartSection(title: String(localized: "Memory Usage")) {
            Chart(data, id: \.date) { point in
                if let percent = point.record.memoryPercent {
                    AreaMark(x: .value("Time", point.date), y: .value("Memory %", percent))
                        .foregroundStyle(gradient(.memoryColor))
                        .interpolationMethod(.catmullRom)
                    LineMark(x: .value("Time", point.date), y: .value("Memory %", percent))
                        .foregroundStyle(Color.memoryColor)
                        .interpolationMethod(.catmullRom)
                }
            }
            .percentageYAxis()
            .timeXAxis()
        }
    }

    private func diskChart(data: [ChartDataPoint]) -> some View {
        ChartSection(title: String(localized: "Disk Usage")) {
            Chart(data, id: \.date) { point in
                if let percent = point.record.diskPercent {
                    AreaMark(x: .value("Time", point.date), y: .value("Disk %", percent))
                        .foregroundStyle(gradient(.diskColor))
                        .interpolationMethod(.catmullRom)
                    LineMark(x: .value("Time", point.date), y: .value("Disk %", percent))
                        .foregroundStyle(Color.diskColor)
                        .interpolationMethod(.catmullRom)
                }
            }
            .percentageYAxis()
            .timeXAxis()
        }
    }

    private func loadChart(data: [ChartDataPoint]) -> some View {
        ChartSection(title: String(localized: "Load Average (1m)")) {
            Chart(data, id: \.date) { point in
                if let load = point.record.load1 {
                    LineMark(x: .value("Time", point.date), y: .value("Load", load))
                        .foregroundStyle(Color.warningAmber)
                        .interpolationMethod(.catmullRom)
                }
            }
            .timeXAxis()
        }
    }

    private func networkChart(data: [ChartDataPoint]) -> some View {
        ChartSection(title: String(localized: "Network I/O")) {
            Chart(data, id: \.date) { point in
                if let netIn = point.record.networkIn {
                    LineMark(
                        x: .value("Time", point.date),
                        y: .value("Bytes/s", netIn),
                        series: .value("Direction", "In")
                    )
                    .foregroundStyle(Color.networkColor)
                    .interpolationMethod(.catmullRom)
                }
                if let netOut = point.record.networkOut {
                    LineMark(
                        x: .value("Time", point.date),
                        y: .value("Bytes/s", netOut),
                        series: .value("Direction", "Out")
                    )
                    .foregroundStyle(Color.cpuColor)
                    .interpolationMethod(.catmullRom)
                }
            }
            .chartForegroundStyleScale([
                "In": Color.networkColor,
                "Out": Color.cpuColor
            ])
            .bytesYAxis()
            .timeXAxis()
        }
    }

    private func diskIOChart(data: [ChartDataPoint]) -> some View {
        ChartSection(title: String(localized: "Disk I/O")) {
            Chart(data, id: \.date) { point in
                if let read = point.record.diskReadPerSec {
                    LineMark(
                        x: .value("Time", point.date),
                        y: .value("Bytes/s", read),
                        series: .value("Direction", "Read")
                    )
                    .foregroundStyle(Color.diskColor)
                    .interpolationMethod(.catmullRom)
                }
                if let write = point.record.diskWritePerSec {
                    LineMark(
                        x: .value("Time", point.date),
                        y: .value("Bytes/s", write),
                        series: .value("Direction", "Write")
                    )
                    .foregroundStyle(Color.warningAmber)
                    .interpolationMethod(.catmullRom)
                }
            }
            .chartForegroundStyleScale([
                "Read": Color.diskColor,
                "Write": Color.warningAmber
            ])
            .bytesYAxis()
            .timeXAxis()
        }
    }

    private func gradient(_ color: Color) -> LinearGradient {
        .linearGradient(
            colors: [color.opacity(0.3), color.opacity(0.05)],
            startPoint: .top,
            endPoint: .bottom
        )
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

private struct BytesYAxisModifier: ViewModifier {
    func body(content: Content) -> some View {
        content.chartYAxis {
            AxisMarks { value in
                AxisGridLine()
                AxisValueLabel {
                    if let bytes = value.as(Double.self) {
                        Text(Formatters.formatSpeed(Int64(bytes)))
                    }
                }
            }
        }
    }
}

extension View {
    func percentageYAxis() -> some View { modifier(PercentageYAxisModifier()) }
    func timeXAxis() -> some View { modifier(TimeXAxisModifier()) }
    func bytesYAxis() -> some View { modifier(BytesYAxisModifier()) }
}
