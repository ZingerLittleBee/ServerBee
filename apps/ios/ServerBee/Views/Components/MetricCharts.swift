import Charts
import SwiftUI

// MARK: - Chart card

/// Legend entry drawn beneath a `ChartSection` chart: a short colour swatch
/// followed by a caption label.
struct ChartLegendItem: Identifiable {
    let label: String
    let color: Color

    var id: String { label }
}

/// A reusable card for a chart: a title with an optional trailing summary
/// (e.g. "avg 21.8% · max 34.1%"), the chart at a fixed height, and an
/// optional legend row underneath.
struct ChartSection<Content: View>: View {
    let title: String
    let subtitle: String?
    let legend: [ChartLegendItem]
    let height: CGFloat
    let content: Content

    init(
        title: String,
        subtitle: String? = nil,
        legend: [ChartLegendItem] = [],
        height: CGFloat = 200,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.subtitle = subtitle
        self.legend = legend
        self.height = height
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            header
            content
                .frame(height: height)
            if !legend.isEmpty {
                ChartLegendRow(items: legend)
            }
        }
        .cardSurface()
        .accessibilityElement(children: .contain)
    }

    /// Title and summary share one baseline; at large Dynamic Type sizes the
    /// summary drops below the title instead of truncating.
    private var header: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                titleLabel
                Spacer(minLength: 8)
                summaryLabel(wraps: false)
            }
            VStack(alignment: .leading, spacing: 2) {
                titleLabel
                summaryLabel(wraps: true)
            }
        }
    }

    private var titleLabel: some View {
        Text(title)
            .font(.subheadline.weight(.semibold))
            .accessibilityAddTraits(.isHeader)
    }

    @ViewBuilder
    private func summaryLabel(wraps: Bool) -> some View {
        if let subtitle {
            Text(subtitle)
                .font(.footnote)
                .foregroundStyle(.secondary)
                .monospacedDigit()
                .lineLimit(wraps ? nil : 1)
                .fixedSize(horizontal: false, vertical: wraps)
        }
    }
}

/// Legend of colour swatches used under multi-series charts: one row, or a
/// column when the labels no longer fit (large Dynamic Type sizes).
private struct ChartLegendRow: View {
    let items: [ChartLegendItem]

    @ScaledMetric(relativeTo: .caption) private var swatchWidth: CGFloat = 10
    @ScaledMetric(relativeTo: .caption) private var swatchHeight: CGFloat = 3

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 14) { entries }
            VStack(alignment: .leading, spacing: 4) { entries }
        }
    }

    private var entries: some View {
        ForEach(items) { item in
            HStack(spacing: 6) {
                Capsule()
                    .fill(item.color)
                    .frame(width: swatchWidth, height: swatchHeight)
                    .accessibilityHidden(true)
                Text(item.label)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

// MARK: - Metric history chart

/// (date, record) pair with a guaranteed-parsable timestamp.
struct ChartDataPoint {
    let date: Date
    let record: MetricRecord
}

/// A single value on a metric history series.
struct MetricChartSample {
    let date: Date
    let value: Double
}

/// One plotted line on a metric history chart. `filled` adds the light area
/// wash under the line (used for the primary series only).
struct MetricChartSeries: Identifiable {
    let name: String
    let color: Color
    let samples: [MetricChartSample]
    var filled = false

    var id: String { name }
}

/// Value formatting and axis scaling for a metric chart's trailing Y axis.
enum MetricAxisFormat {
    case percent
    case bytes
    case bytesPerSecond
    case decimal
    case celsius

    /// Compact axis label; zero is always rendered as a bare "0".
    func label(for value: Double) -> String {
        guard value != 0 else { return "0" }
        switch self {
        case .percent: return "\(Int(value.rounded()))%"
        case .bytes: return Formatters.formatBytes(Int64(value))
        case .bytesPerSecond: return Formatters.formatSpeed(Int64(value))
        case .decimal: return value.formatted(.number.precision(.fractionLength(0...2)))
        case .celsius: return "\(Int(value.rounded()))°"
        }
    }

    /// Readable axis top for a series maximum, so the three gridlines
    /// (0, mid, top) land on round values. Keeps a little headroom so a peak
    /// never sits exactly on the top gridline.
    func axisTop(for maxValue: Double) -> Double {
        let padded = maxValue * 1.05
        switch self {
        case .percent:
            return 100
        case .celsius:
            return max(100, (padded / 10).rounded(.up) * 10)
        case .decimal:
            return Self.niceCeiling(max(padded, 1))
        case .bytes, .bytesPerSecond:
            return Self.niceBinaryCeiling(padded)
        }
    }

    /// Rounds up to the next 1 / 2 / 2.5 / 5 step of the value's magnitude.
    private static func niceCeiling(_ value: Double) -> Double {
        guard value > 0, value.isFinite else { return 1 }
        let magnitude = pow(10, floor(log10(value)))
        let fraction = value / magnitude
        let step = [1, 2, 2.5, 5, 10].first { fraction <= $0 } ?? 10
        return step * magnitude
    }

    /// Nice ceiling within the binary unit (KB / MB / GB …) the value falls
    /// in, so byte labels read "5 MB" rather than "4.77 MB". Minimum 1 KB.
    private static func niceBinaryCeiling(_ value: Double) -> Double {
        guard value > 1_024, value.isFinite else { return 1_024 }
        var unit: Double = 1
        while value / unit >= 1_024 { unit *= 1_024 }
        let scaled = value / unit
        return scaled > 500 ? 1_024 * unit : niceCeiling(scaled) * unit
    }
}

/// Standard metric history chart: trailing Y axis with three gridlines
/// (0, mid, top), three compact time labels pinned to the plot edges, a light
/// area wash under filled series and 2pt lines.
struct MetricHistoryChart: View {
    /// Accessibility name for the plotted values (usually the card title).
    let valueLabel: String
    let series: [MetricChartSeries]
    let format: MetricAxisFormat
    let timeDomain: ClosedRange<Date>

    private var peak: Double {
        series.flatMap(\.samples).map(\.value).max() ?? 0
    }

    var body: some View {
        let peak = peak
        let top = format.axisTop(for: peak)
        let format = format
        Chart {
            ForEach(series) { line in
                ForEach(line.samples, id: \.date) { sample in
                    if line.filled {
                        AreaMark(
                            x: .value("Time", sample.date),
                            y: .value(valueLabel, sample.value),
                            series: .value(valueLabel, line.name),
                            stacking: .unstacked
                        )
                        .foregroundStyle(line.color.opacity(0.14))
                        .interpolationMethod(.monotone)
                    }
                    LineMark(
                        x: .value("Time", sample.date),
                        y: .value(valueLabel, sample.value),
                        series: .value(valueLabel, line.name)
                    )
                    .foregroundStyle(line.color)
                    .lineStyle(StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
                    .interpolationMethod(.monotone)
                }
            }
        }
        .chartLegend(.hidden)
        .chartXScale(domain: timeDomain)
        .chartYScale(domain: 0...max(top, peak))
        .chartYAxis {
            AxisMarks(position: .trailing, values: [0, top / 2, top]) { value in
                AxisGridLine()
                AxisValueLabel {
                    if let number = value.as(Double.self) {
                        Text(verbatim: format.label(for: number))
                    }
                }
            }
        }
        .edgeTimeXAxis(domain: timeDomain)
        // Axis labels stop growing at the first accessibility size so the
        // plot keeps usable width; card titles and summaries still scale.
        .dynamicTypeSize(...DynamicTypeSize.accessibility1)
    }
}

// MARK: - Shared Chart Axis Modifiers

/// Three compact time labels without vertical gridlines.
private struct TimeXAxisModifier: ViewModifier {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    func body(content: Content) -> some View {
        // Three clock labels collide at accessibility text sizes.
        content
            .chartXAxis {
                AxisMarks(values: .automatic(desiredCount: dynamicTypeSize.isAccessibilitySize ? 2 : 3)) { value in
                    AxisValueLabel {
                        if let date = value.as(Date.self) {
                            Text(verbatim: Formatters.formatChartTime(date))
                        }
                    }
                }
            }
    }
}

/// Start / middle / end time labels pinned inside the plot edges (start / end
/// only at accessibility text sizes, where three labels collide). Spans longer
/// than a day and a half switch from "HH:mm" to "M/d" labels.
private struct EdgeTimeXAxisModifier: ViewModifier {
    let domain: ClosedRange<Date>

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    func body(content: Content) -> some View {
        let start = domain.lowerBound
        let end = domain.upperBound
        let middle = start.addingTimeInterval(end.timeIntervalSince(start) / 2)
        let showsDates = end.timeIntervalSince(start) > 36 * 3_600
        let values = dynamicTypeSize.isAccessibilitySize ? [start, end] : [start, middle, end]
        return content
            .chartXAxis {
                AxisMarks(values: values) { value in
                    AxisValueLabel(anchor: Self.anchor(index: value.index, count: value.count)) {
                        if let date = value.as(Date.self) {
                            Text(verbatim: showsDates ? Formatters.formatDayAxis(date) : Formatters.formatChartTime(date))
                        }
                    }
                }
            }
    }

    private static func anchor(index: Int, count: Int) -> UnitPoint {
        if index == 0 { return .topLeading }
        if index == count - 1 { return .topTrailing }
        return .top
    }
}

extension View {
    func timeXAxis() -> some View { modifier(TimeXAxisModifier()) }
    func edgeTimeXAxis(domain: ClosedRange<Date>) -> some View { modifier(EdgeTimeXAxisModifier(domain: domain)) }
}
