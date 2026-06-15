import Charts
import SwiftUI

// MARK: - Traffic Cycle Card

/// Current billing-cycle usage with a limit progress bar and end-of-cycle
/// projection (when a limit is configured).
struct TrafficCycleCard: View {
    let traffic: TrafficResponse

    var body: some View {
        SectionCard(String(localized: "This Cycle"), systemImage: "calendar") {
            VStack(alignment: .leading, spacing: 14) {
                Text("\(traffic.cycleStart) → \(traffic.cycleEnd)")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                if let limit = traffic.trafficLimit, limit > 0 {
                    limitSection(limit: limit)
                }

                Divider()

                DetailRow(
                    label: String(localized: "Download"),
                    value: Formatters.formatBytes(traffic.bytesIn),
                    systemImage: "arrow.down",
                    valueColor: .networkColor
                )
                DetailRow(
                    label: String(localized: "Upload"),
                    value: Formatters.formatBytes(traffic.bytesOut),
                    systemImage: "arrow.up",
                    valueColor: .cpuColor
                )
                DetailRow(
                    label: String(localized: "Total"),
                    value: Formatters.formatBytes(traffic.bytesTotal),
                    systemImage: "sum"
                )

                if let prediction = traffic.prediction {
                    Divider()
                    predictionRow(prediction)
                }
            }
        }
    }

    @ViewBuilder
    private func limitSection(limit: Int64) -> some View {
        let fraction = Double(traffic.countedBytes) / Double(limit)
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(Formatters.formatBytes(traffic.countedBytes))
                    .font(.subheadline.bold())
                Text("/ \(Formatters.formatBytes(limit))")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                if let label = traffic.limitTypeLabel {
                    Chip(text: label, color: .secondary)
                }
                Spacer()
                Text(String(format: "%.1f%%", (traffic.usagePercent ?? fraction * 100)))
                    .font(.subheadline.bold())
                    .foregroundStyle(usageColor(fraction))
            }
            UsageBar(value: fraction)
        }
    }

    private func predictionRow(_ prediction: TrafficPrediction) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: prediction.willExceed ? "exclamationmark.triangle.fill" : "chart.line.uptrend.xyaxis")
                .font(.caption)
                .foregroundStyle(prediction.willExceed ? Color.serverOffline : .secondary)
                .frame(width: 18)
            Text(String(localized: "Projected end of cycle"))
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Spacer(minLength: 12)
            VStack(alignment: .trailing, spacing: 2) {
                Text(Formatters.formatBytes(prediction.estimatedTotal))
                    .font(.subheadline.bold())
                Text(String(format: "%.0f%%", prediction.estimatedPercent))
                    .font(.caption)
                    .foregroundStyle(prediction.willExceed ? Color.serverOffline : .secondary)
            }
        }
    }

    private func usageColor(_ fraction: Double) -> Color {
        switch fraction {
        case ..<0.7: .serverOnline
        case ..<0.9: .warningAmber
        default: .serverOffline
        }
    }
}

// MARK: - Daily Traffic Chart

/// Stacked daily in/out bytes across the billing cycle.
struct TrafficDailyChart: View {
    let daily: [DailyTraffic]

    private struct Bar: Identifiable {
        let id: String
        let date: Date
        let bytes: Int64
        let direction: String
    }

    private var bars: [Bar] {
        daily.flatMap { day -> [Bar] in
            guard let date = Formatters.parseDay(day.date) else { return [] }
            return [
                Bar(id: "\(day.date)-in", date: date, bytes: day.bytesIn, direction: String(localized: "Download")),
                Bar(id: "\(day.date)-out", date: date, bytes: day.bytesOut, direction: String(localized: "Upload"))
            ]
        }
    }

    var body: some View {
        ChartSection(title: String(localized: "Daily Traffic")) {
            Chart(bars) { bar in
                BarMark(
                    x: .value("Day", bar.date, unit: .day),
                    y: .value("Bytes", bar.bytes)
                )
                .foregroundStyle(by: .value("Direction", bar.direction))
            }
            .chartForegroundStyleScale([
                String(localized: "Download"): Color.networkColor,
                String(localized: "Upload"): Color.cpuColor
            ])
            .chartLegend(position: .top, alignment: .leading)
            .chartYAxis {
                AxisMarks { value in
                    AxisGridLine()
                    AxisValueLabel {
                        if let bytes = value.as(Double.self) {
                            Text(Formatters.formatBytes(Int64(bytes)))
                        }
                    }
                }
            }
            .chartXAxis {
                AxisMarks(values: .stride(by: .day, count: max(1, daily.count / 6))) { value in
                    AxisGridLine()
                    AxisValueLabel {
                        if let date = value.as(Date.self) {
                            Text(Formatters.formatDayAxis(date))
                        }
                    }
                }
            }
        }
    }
}

// MARK: - Uptime Card

/// 90-day uptime timeline with overall ratio and tap-to-inspect a day.
struct UptimeCard: View {
    let days: [UptimeDailyEntry]
    let windowDays: Int

    @State private var selected: UptimeDailyEntry?

    var body: some View {
        SectionCard(String(localized: "Uptime"), systemImage: "checkmark.shield") {
            if days.isEmpty {
                Text(String(localized: "No uptime data yet"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            } else {
                VStack(alignment: .leading, spacing: 12) {
                    summary
                    UptimeTimelineBar(days: days, selectedDate: selected?.date) { day in
                        selected = (selected?.id == day.id) ? nil : day
                    }
                    if let selected {
                        selectedRow(selected)
                    }
                    UptimeLegend()
                }
            }
        }
    }

    private var summary: some View {
        HStack(alignment: .firstTextBaseline) {
            if let ratio = days.overallRatio {
                Text(String(format: "%.2f%%", ratio * 100))
                    .font(.title2.bold())
                    .foregroundStyle(ratio >= 0.99 ? Color.serverOnline : .warningAmber)
            }
            Text(String(localized: "over \(windowDays) days"))
                .font(.caption)
                .foregroundStyle(.secondary)
            Spacer()
            if days.totalIncidents > 0 {
                Chip(
                    text: String(localized: "\(days.totalIncidents) incidents"),
                    systemImage: "bolt.trianglebadge.exclamationmark",
                    color: .warningAmber
                )
            }
        }
    }

    private func selectedRow(_ day: UptimeDailyEntry) -> some View {
        HStack(spacing: 10) {
            Text(day.date)
                .font(.subheadline.bold())
            Spacer()
            if let ratio = day.ratio {
                Text(String(format: "%.1f%%", ratio * 100))
                    .font(.subheadline)
                    .foregroundStyle(colorForStatus(day.status))
            } else {
                Text(String(localized: "No data"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            if day.downtimeIncidents > 0 {
                Text(String(localized: "\(day.downtimeIncidents) incidents"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(10)
        .background(Color(.systemGray6))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private func colorForStatus(_ status: UptimeStatus) -> Color {
        switch status {
        case .operational: .serverOnline
        case .degraded: .warningAmber
        case .down: .serverOffline
        case .noData: .secondary
        }
    }
}
