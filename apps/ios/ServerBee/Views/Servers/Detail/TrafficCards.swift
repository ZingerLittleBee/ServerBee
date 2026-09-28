import Charts
import SwiftUI

// MARK: - Traffic Cycle Card

/// Current billing-cycle usage: headline total, limit bar, per-direction
/// totals and the end-of-cycle projection (when a limit is configured).
struct TrafficCycleCard: View {
    let traffic: TrafficResponse

    @ScaledMetric(relativeTo: .body) private var barHeight: CGFloat = 10
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    /// Configured limit, ignoring a non-positive value.
    private var limit: Int64? {
        guard let limit = traffic.trafficLimit, limit > 0 else { return nil }
        return limit
    }

    /// The headline figure: bytes counted against the limit, or the cycle
    /// total when no limit is set.
    private var headlineBytes: Int64 {
        limit == nil ? traffic.bytesTotal : traffic.countedBytes
    }

    var body: some View {
        SectionCard(String(localized: "This cycle")) {
            VStack(alignment: .leading, spacing: 12) {
                usedRow
                if let limit {
                    limitSection(limit: limit)
                }
                totals
                if let prediction = traffic.prediction {
                    Divider()
                    predictionRow(prediction)
                }
            }
        } accessory: {
            Text(verbatim: "\(TrafficDayFormat.short(day: traffic.cycleStart)) → \(TrafficDayFormat.short(day: traffic.cycleEnd))")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .monospacedDigit()
                .multilineTextAlignment(.trailing)
        }
    }
}

private extension TrafficCycleCard {
    var usedRow: some View {
        // At large Dynamic Type sizes the muted "/ limit" drops below the
        // headline instead of truncating.
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .firstTextBaseline, spacing: 4) {
                usedText
                limitText
            }
            VStack(alignment: .leading, spacing: 2) {
                usedText
                limitText
            }
        }
        .monospacedDigit()
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(limit == nil ? String(localized: "Total") : traffic.limitTypeLabel ?? String(localized: "Total")))
        .accessibilityValue(Text(verbatim: usedAccessibilityValue))
    }

    var usedText: some View {
        Text(Formatters.formatBytes(headlineBytes))
            .font(.largeTitle.bold())
            .lineLimit(1)
            .minimumScaleFactor(0.6)
    }

    @ViewBuilder
    var limitText: some View {
        if let limit {
            Text(verbatim: "/ \(Formatters.formatBytes(limit))")
                .font(.body.weight(.medium))
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
    }

    var usedAccessibilityValue: String {
        let used = Formatters.formatBytes(headlineBytes)
        guard let limit else { return used }
        return "\(used) / \(Formatters.formatBytes(limit))"
    }

    func limitSection(limit: Int64) -> some View {
        let fraction = Double(traffic.countedBytes) / Double(limit)
        return VStack(alignment: .leading, spacing: 8) {
            UsageBar(value: fraction, height: barHeight, tint: barColor(fraction))
                .accessibilityLabel(Text(String(localized: "Traffic limit")))
            HStack(spacing: 8) {
                if let label = traffic.limitTypeLabel {
                    Chip(text: label, color: .secondary)
                }
                Spacer(minLength: 8)
                Text(String(format: "%.1f%%", traffic.usagePercent ?? fraction * 100))
                    .font(.footnote.weight(.semibold))
                    .monospacedDigit()
                    .foregroundStyle(percentColor(fraction))
            }
        }
    }

    var totals: some View {
        // Three single-line columns cannot fit accessibility text sizes; stack
        // them the way the old label/value rows did.
        let layout = dynamicTypeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 10))
            : AnyLayout(HStackLayout(alignment: .top, spacing: 12))
        return layout {
            TrafficStatCell(
                label: String(localized: "Download"),
                value: Formatters.formatBytes(traffic.bytesIn),
                systemImage: "arrow.down",
                iconColor: .networkColor
            )
            TrafficStatCell(
                label: String(localized: "Upload"),
                value: Formatters.formatBytes(traffic.bytesOut),
                systemImage: "arrow.up",
                iconColor: .trafficUpload
            )
            // The headline already shows the total unless the limit counts a
            // single direction.
            if headlineBytes != traffic.bytesTotal {
                TrafficStatCell(
                    label: String(localized: "Total"),
                    value: Formatters.formatBytes(traffic.bytesTotal),
                    systemImage: "sum"
                )
            }
        }
    }

    func predictionRow(_ prediction: TrafficPrediction) -> some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                predictionLabel(prediction)
                Spacer(minLength: 8)
                predictionValue(prediction)
            }
            VStack(alignment: .leading, spacing: 4) {
                predictionLabel(prediction)
                predictionValue(prediction)
            }
        }
        .accessibilityElement(children: .combine)
    }

    func predictionLabel(_ prediction: TrafficPrediction) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(String(localized: "Projected end of cycle"))
                .font(.subheadline)
            Text(String(localized: "\(String(format: "%.0f%%", prediction.estimatedPercent)) of limit"))
                .font(.footnote)
                .foregroundStyle(.secondary)
                .monospacedDigit()
        }
    }

    func predictionValue(_ prediction: TrafficPrediction) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 5) {
            Text(Formatters.formatBytes(prediction.estimatedTotal))
                .font(.subheadline.weight(.semibold))
                .monospacedDigit()
                .lineLimit(1)
            Text(prediction.willExceed ? String(localized: "over limit") : String(localized: "within limit"))
                .font(.footnote.weight(.medium))
                .foregroundStyle(prediction.willExceed ? Color.serverOffline : .serverOnline)
        }
    }

    /// Bar fill: the metric colour until the limit gets close.
    func barColor(_ fraction: Double) -> Color {
        switch fraction {
        case ..<0.7: .networkColor
        case ..<0.9: .warningAmber
        default: .serverOffline
        }
    }

    func percentColor(_ fraction: Double) -> Color {
        fraction < 0.7 ? .secondary : barColor(fraction)
    }
}

// MARK: - Daily Traffic Chart

/// Stacked daily in/out bytes (download at the base, upload on top) across the
/// billing cycle, or the fleet-wide window on the Insights traffic screen.
struct TrafficDailyChart: View {
    let daily: [DailyTraffic]

    @ScaledMetric(relativeTo: .body) private var chartHeight: CGFloat = 150
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    fileprivate struct Segment: Identifiable {
        let id: String
        let date: Date
        let start: Double
        let end: Double
        let bytes: Int64
        let direction: String
    }

    /// One x-axis label: where it sits on the date scale and how it anchors.
    fileprivate struct AxisLabel {
        let position: Date
        let text: String
        let anchor: UnitPoint
    }

    private var downloadLabel: String { String(localized: "Download") }
    private var uploadLabel: String { String(localized: "Upload") }

    var body: some View {
        SectionCard(String(localized: "Daily traffic")) {
            VStack(alignment: .leading, spacing: 10) {
                chart
                    // Scale with Dynamic Type, but stop before the chart
                    // dominates the screen at accessibility sizes.
                    .frame(height: min(chartHeight, 240))
                legend
            }
        } accessory: {
            if let caption = TrafficDayFormat.rangeCaption(days: daily.map(\.date)) {
                Text(caption)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
        }
    }
}

private extension TrafficDailyChart {
    var chart: some View {
        let scale = TrafficAxisScale(maxBytes: daily.map(\.bytesTotal).max() ?? 0)
        let labels = axisLabels
        return Chart(segments(scale: scale)) { segment in
            BarMark(
                x: .value("Day", segment.date, unit: .day),
                yStart: .value("Bytes", segment.start),
                yEnd: .value("Bytes", segment.end),
                width: .ratio(0.65)
            )
            .foregroundStyle(by: .value("Direction", segment.direction))
            .cornerRadius(2)
            .accessibilityLabel(Text(verbatim: "\(TrafficDayFormat.short(segment.date)), \(segment.direction)"))
            .accessibilityValue(Text(Formatters.formatBytes(segment.bytes)))
        }
        .chartForegroundStyleScale([
            downloadLabel: Color.networkColor,
            uploadLabel: Color.trafficUpload
        ])
        .chartLegend(.hidden)
        .chartYScale(domain: 0...scale.upperBound)
        .chartYAxis {
            AxisMarks(position: .trailing, values: scale.ticks) { value in
                AxisGridLine()
                AxisValueLabel {
                    if let bytes = value.as(Double.self) {
                        Text(verbatim: scale.label(for: bytes))
                    }
                }
            }
        }
        .chartXAxis {
            AxisMarks(values: labels.map(\.position)) { value in
                AxisValueLabel(anchor: labels.indices.contains(value.index) ? labels[value.index].anchor : .top) {
                    if labels.indices.contains(value.index) {
                        Text(verbatim: labels[value.index].text)
                    }
                }
            }
        }
        .environment(\.calendar, TrafficDayFormat.utcCalendar)
        .environment(\.timeZone, .gmt)
    }

    /// Manually stacked segments so upload sits on download with a hairline
    /// gap; the gap is taken out of the upload segment so the bar top stays
    /// at the true daily total.
    func segments(scale: TrafficAxisScale) -> [Segment] {
        let gap = scale.upperBound * 0.012
        return daily.flatMap { day -> [Segment] in
            guard let date = Formatters.parseDay(day.date) else { return [] }
            let down = Double(day.bytesIn)
            let up = Double(day.bytesOut)
            let upStart = down > 0 ? down + min(gap, up / 2) : 0
            return [
                Segment(id: "\(day.date)-in", date: date, start: 0, end: down, bytes: day.bytesIn, direction: downloadLabel),
                Segment(id: "\(day.date)-out", date: date, start: upStart, end: down + up, bytes: day.bytesOut, direction: uploadLabel)
            ]
        }
    }

    /// First, middle and last day. With the UTC chart calendar a day's value
    /// lands on its bar's centre, so the middle label is centred there. A
    /// centred label on the first or last bar would overflow the plot and be
    /// truncated ("S…"), so those two are pinned flush to the plot edges
    /// instead: half a day outside the bar centre, anchored leading/trailing.
    var axisLabels: [AxisLabel] {
        let dates = daily.compactMap { Formatters.parseDay($0.date) }
        guard let first = dates.first, let last = dates.last else { return [] }
        guard dates.count > 1 else {
            return [AxisLabel(position: first, text: TrafficDayFormat.short(first), anchor: .top)]
        }
        let halfDay: TimeInterval = 12 * 60 * 60
        var labels = [AxisLabel(position: first.addingTimeInterval(-halfDay), text: TrafficDayFormat.short(first), anchor: .topLeading)]
        // Accessibility text sizes only leave room for the two edge labels.
        if dates.count > 2, !dynamicTypeSize.isAccessibilitySize {
            let middle = dates[dates.count / 2]
            labels.append(AxisLabel(position: middle, text: TrafficDayFormat.short(middle), anchor: .top))
        }
        labels.append(AxisLabel(position: last.addingTimeInterval(halfDay), text: TrafficDayFormat.short(last), anchor: .topTrailing))
        return labels
    }

    var legend: some View {
        HStack(spacing: 14) {
            legendItem(color: .networkColor, label: downloadLabel)
            legendItem(color: .trafficUpload, label: uploadLabel)
        }
        .font(.caption)
        .foregroundStyle(.secondary)
    }

    func legendItem(color: Color, label: String) -> some View {
        HStack(spacing: 6) {
            Capsule()
                .fill(color)
                .frame(width: 10, height: 3)
                .accessibilityHidden(true)
            Text(label)
        }
    }
}

// MARK: - Uptime Card

/// Uptime timeline with the overall ratio and tap-to-inspect a day.
struct UptimeCard: View {
    let days: [UptimeDailyEntry]
    let windowDays: Int

    @State private var selected: UptimeDailyEntry?

    var body: some View {
        SectionCard(String(localized: "Uptime")) {
            if days.isEmpty {
                Text(String(localized: "No uptime data yet"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            } else {
                VStack(alignment: .leading, spacing: 10) {
                    UptimeTimelineBar(days: days, selectedDate: selected?.date) { day in
                        selected = (selected?.id == day.id) ? nil : day
                    }
                    if let selected {
                        selectedRow(selected)
                    }
                    footnote
                    UptimeLegend()
                }
            }
        } accessory: {
            if let ratio = days.overallRatio {
                Text(String(format: "%.2f%%", ratio * 100))
                    .font(.subheadline.weight(.semibold))
                    .monospacedDigit()
                    .foregroundStyle(Self.ratioColor(days.overallStatus))
            }
        }
    }
}

private extension UptimeCard {
    static func ratioColor(_ status: UptimeStatus) -> Color {
        switch status {
        case .operational: .serverOnline
        case .degraded: .warningAmber
        case .down: .serverOffline
        case .noData: .secondary
        }
    }

    var footnote: some View {
        HStack(spacing: 4) {
            Text(String(localized: "over \(windowDays) days"))
            if days.totalIncidents > 0 {
                Text(verbatim: "·")
                Text(incidentsText(days.totalIncidents))
            }
        }
        .font(.footnote)
        .foregroundStyle(.secondary)
        .monospacedDigit()
        .accessibilityElement(children: .combine)
    }

    func selectedRow(_ day: UptimeDailyEntry) -> some View {
        HStack(spacing: 10) {
            Text(TrafficDayFormat.full(day: day.date))
                .font(.subheadline.weight(.semibold))
            Spacer()
            if let ratio = day.ratio {
                Text(String(format: "%.1f%%", ratio * 100))
                    .font(.subheadline)
                    .monospacedDigit()
                    .foregroundStyle(colorForStatus(day.status))
            } else {
                Text(String(localized: "No data"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            if day.downtimeIncidents > 0 {
                Text(incidentsText(day.downtimeIncidents))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(Color(.tertiarySystemFill), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .accessibilityElement(children: .combine)
    }

    /// "1 incident" / "3 incidents" (the catalog key has no plural variants).
    func incidentsText(_ count: Int) -> String {
        count == 1 ? String(localized: "1 incident") : String(localized: "\(count) incidents")
    }

    func colorForStatus(_ status: UptimeStatus) -> Color {
        switch status {
        case .operational: .serverOnline
        case .degraded: .warningAmber
        case .down: .serverOffline
        case .noData: .secondary
        }
    }
}
