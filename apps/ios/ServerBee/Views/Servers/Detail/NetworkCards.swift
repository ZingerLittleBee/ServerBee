import Charts
import SwiftUI

// MARK: - Probe health

/// Probe health summary: 24h anomaly badge, server online state, assigned
/// target count and the last probe time.
struct NetworkSummaryCard: View {
    let summary: NetworkProbeServerSummary
    let targetCount: Int

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    private var statusColor: Color { summary.online ? .serverOnline : .serverOffline }
    private var statusLabel: String {
        summary.online ? String(localized: "Online") : String(localized: "Offline")
    }

    private var detailLine: String {
        var parts = [
            targetCount == 1
                ? String(localized: "1 target")
                : String(localized: "\(targetCount) targets")
        ]
        if let last = summary.lastProbeAt {
            parts.append(String(localized: "last probe \(Formatters.formatRelativeTime(last))"))
        }
        return parts.joined(separator: " · ")
    }

    var body: some View {
        let isAccessibilitySize = dynamicTypeSize.isAccessibilitySize
        let titleLayout = isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 6))
            : AnyLayout(HStackLayout(alignment: .firstTextBaseline, spacing: 8))
        VStack(alignment: .leading, spacing: 8) {
            titleLayout {
                Text(String(localized: "Probe health"))
                    .font(.subheadline.weight(.semibold))
                if !isAccessibilitySize {
                    Spacer(minLength: 8)
                }
                healthBadge
            }
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Image(systemName: "circle.fill")
                    .font(.caption2)
                    .imageScale(.small)
                    .foregroundStyle(statusColor)
                    .accessibilityHidden(true)
                Text(statusLabel)
                    .fontWeight(.semibold)
                    .foregroundStyle(statusColor)
                Text(verbatim: "·")
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                Text(detailLine)
                    .foregroundStyle(.secondary)
            }
            .font(.footnote)
            .monospacedDigit()
        }
        .cardSurface()
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private var healthBadge: some View {
        switch summary.anomalyCount {
        case 0:
            // "Healthy" only means something while probes are actually running.
            if summary.online, targetCount > 0 {
                StatusBadge(text: String(localized: "Healthy"), color: .serverOnline)
            }
        case 1:
            StatusBadge(text: String(localized: "1 anomaly · 24h"), color: .warningAmber)
        default:
            StatusBadge(text: String(localized: "\(summary.anomalyCount) anomalies · 24h"), color: .warningAmber)
        }
    }
}

// MARK: - Latency chart

/// Average latency over time, one coloured line per target, with a legend
/// below the plot.
struct NetworkLatencyChart: View {
    let records: [ProbeRecordDto]
    let targets: [NetworkProbeTarget]
    let palette: NetworkTargetPalette
    var isLoading = false

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.privacyMode) private var privacyMode
    @ScaledMetric(relativeTo: .body) private var chartHeight: CGFloat = 160

    /// Plot height, capped so accessibility text sizes don't produce a
    /// screen-filling chart.
    private var plotHeight: CGFloat { min(chartHeight, 280) }

    private struct Point: Identifiable {
        let id: String
        let date: Date
        let latency: Double
        let targetId: String
        /// Human series label (target name), used by the legend, VoiceOver and
        /// Audio Graph instead of the (often UUID) target id.
        let series: String
    }

    /// Target name per id, extended with probe type + address when another
    /// assigned target shares that name so the series stay distinct.
    private var seriesLabelByID: [String: String] {
        let nameCounts = Dictionary(targets.map { ($0.name, 1) }, uniquingKeysWith: +)
        return Dictionary(targets.map { target in
            let isShared = (nameCounts[target.name] ?? 0) > 1
            let label = isShared ? "\(target.name) (\(target.probeType.uppercased()) \(target.target.maskingIPs(privacyMode)))" : target.name
            return (target.id, label)
        }, uniquingKeysWith: { a, _ in a })
    }

    private var points: [Point] {
        let labels = seriesLabelByID
        return records.compactMap { rec in
            guard let date = rec.date, let latency = rec.avgLatency else { return nil }
            return Point(
                id: "\(rec.targetId)-\(rec.timestamp)", date: date, latency: latency,
                targetId: rec.targetId, series: labels[rec.targetId] ?? rec.targetId
            )
        }
    }

    /// Distinct probe types of the assigned targets, e.g. "ICMP · ms".
    private var unitCaption: String {
        var types: [String] = []
        for type in targets.map({ $0.probeType.uppercased() }) where !types.contains(type) {
            types.append(type)
        }
        return types.isEmpty ? "ms" : "\(types.joined(separator: "/")) · ms"
    }

    var body: some View {
        let points = points
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Text(String(localized: "Latency"))
                    .font(.subheadline.weight(.semibold))
                Spacer(minLength: 8)
                if isLoading && !points.isEmpty {
                    ProgressView()
                        .controlSize(.small)
                }
                Text(verbatim: unitCaption)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            if points.isEmpty {
                if isLoading {
                    ProgressView()
                        .frame(maxWidth: .infinity, minHeight: plotHeight)
                } else {
                    emptyChart
                }
            } else {
                chart(points)
                legend(points)
            }
        }
        .cardSurface()
    }

    private func chart(_ points: [Point]) -> some View {
        Chart(points) { point in
            LineMark(
                x: .value("Time", point.date),
                y: .value("Latency", point.latency),
                series: .value("Target", point.series)
            )
            .foregroundStyle(palette.color(for: point.targetId))
            .lineStyle(StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
            .interpolationMethod(.catmullRom)
        }
        .chartLegend(.hidden)
        .chartYAxis {
            AxisMarks(position: .trailing, values: .automatic(desiredCount: 3)) { value in
                AxisGridLine()
                AxisValueLabel {
                    if let ms = value.as(Double.self) {
                        Text(verbatim: "\(Int(ms)) ms")
                    }
                }
            }
        }
        .timeXAxis()
        .frame(height: plotHeight)
    }

    /// Wrapping legend; stacks vertically at accessibility sizes because
    /// `WrapLayout` sizes items at their ideal (single-line) width, which would
    /// clip long target names against the card edge.
    private func legend(_ points: [Point]) -> some View {
        let isAccessibilitySize = dynamicTypeSize.isAccessibilitySize
        let labels = Dictionary(points.map { ($0.targetId, $0.series) }, uniquingKeysWith: { a, _ in a })
        let ids = labels.keys.sorted { palette.order(for: $0) < palette.order(for: $1) }
        let layout = isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 6))
            : AnyLayout(WrapLayout(spacing: 14, lineSpacing: 6))
        return layout {
            ForEach(ids, id: \.self) { id in
                HStack(spacing: 6) {
                    Capsule()
                        .fill(palette.color(for: id))
                        .frame(width: 10, height: 3)
                        .accessibilityHidden(true)
                    Text(labels[id] ?? id)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(isAccessibilitySize ? nil : 1)
                }
            }
        }
        .accessibilityElement(children: .combine)
    }

    private var emptyChart: some View {
        ContentUnavailableView(
            String(localized: "No probe data"),
            systemImage: "chart.xyaxis.line",
            description: Text(String(localized: "No samples in this range"))
        )
        .frame(maxWidth: .infinity)
    }
}

// MARK: - Targets

/// "Targets" group: per-provider grouped rows with the chart colour dot,
/// packet loss, probe type + address, and the average latency.
struct NetworkTargetsCard: View {
    let targets: [NetworkProbeTarget]
    let summaries: [TargetSummary]
    let palette: NetworkTargetPalette

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.privacyMode) private var privacyMode
    @ScaledMetric(relativeTo: .body) private var dotSize: CGFloat = 9

    private var summaryByID: [String: TargetSummary] {
        Dictionary(summaries.map { ($0.targetId, $0) }, uniquingKeysWith: { a, _ in a })
    }

    /// Leading inset of row separators: aligns with the target name.
    private var rowInset: CGFloat { 16 + dotSize + 12 }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            GroupHeader(String(localized: "Targets"))
            if targets.isEmpty {
                Text(String(localized: "No probe targets assigned"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .cardSurface()
            } else {
                groupedRows
            }
        }
    }

    private var groupedRows: some View {
        let summaries = summaryByID
        return VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(NetworkTargetGroup.groups(targets).enumerated()), id: \.element.id) { index, group in
                if index > 0 {
                    Divider()
                }
                Text(NetworkProvider.label(for: group.provider))
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 16)
                    .padding(.top, 10)
                    .accessibilityAddTraits(.isHeader)
                ForEach(Array(group.targets.enumerated()), id: \.element.id) { rowIndex, target in
                    if rowIndex > 0 {
                        Divider()
                            .padding(.leading, rowInset)
                    }
                    targetRow(target, summary: summaries[target.id])
                }
            }
        }
        .cardSurface(padding: 0)
    }
}

private extension NetworkTargetsCard {
    func targetRow(_ target: NetworkProbeTarget, summary: TargetSummary?) -> some View {
        let isAccessibilitySize = dynamicTypeSize.isAccessibilitySize
        let contentLayout = isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 4))
            : AnyLayout(HStackLayout(spacing: 8))
        return HStack(spacing: 12) {
            Circle()
                .fill(palette.color(for: target.id))
                .frame(width: dotSize, height: dotSize)
                .accessibilityHidden(true)
            contentLayout {
                VStack(alignment: .leading, spacing: 1) {
                    Text(target.name)
                        .lineLimit(isAccessibilitySize ? nil : 2)
                    detailLine(target, summary: summary, stacked: isAccessibilitySize)
                }
                .fixedSize(horizontal: false, vertical: true)
                if !isAccessibilitySize {
                    Spacer(minLength: 0)
                }
                latencyValue(summary?.avgLatency)
                    .layoutPriority(1)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
        .accessibilityElement(children: .combine)
    }

    /// Loss + probe type/address. `stacked` (accessibility sizes) puts them
    /// on separate wrapping lines instead of truncating the address.
    func detailLine(_ target: NetworkProbeTarget, summary: TargetSummary?, stacked: Bool) -> some View {
        let layout = stacked
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 1))
            : AnyLayout(HStackLayout(spacing: 4))
        return layout {
            if let loss = summary?.packetLoss {
                Text(String(localized: "loss \(NetworkFormat.loss(loss))"))
                    .foregroundStyle(lossColor(loss))
                    .fixedSize(horizontal: !stacked, vertical: false)
                if !stacked {
                    Text(verbatim: "·")
                        .accessibilityHidden(true)
                }
            }
            Text(verbatim: "\(target.probeType.uppercased()) · \(target.target.maskingIPs(privacyMode))")
                .lineLimit(stacked ? nil : 1)
                .truncationMode(.middle)
        }
        .font(.footnote)
        .foregroundStyle(.secondary)
        .monospacedDigit()
    }

    @ViewBuilder
    func latencyValue(_ ms: Double?) -> some View {
        if let ms {
            HStack(alignment: .firstTextBaseline, spacing: 2) {
                Text(verbatim: ms < 10 ? String(format: "%.1f", ms) : String(format: "%.0f", ms))
                    .font(.headline)
                    .foregroundStyle(latencyColor(ms))
                Text(verbatim: "ms")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            .monospacedDigit()
        } else {
            Text(verbatim: "—")
                .font(.headline)
                .foregroundStyle(.tertiary)
        }
    }

    /// Normal latency stays primary; elevated values escalate amber → red.
    func latencyColor(_ ms: Double) -> Color {
        switch ms {
        case ..<100: return .primary
        case ..<300: return .warningAmber
        default: return .serverOffline
        }
    }

    /// Anything that renders as "0.0%" stays neutral.
    func lossColor(_ ratio: Double) -> Color {
        switch ratio {
        case ..<0.0005: return .secondary
        case ..<0.1: return .warningAmber
        default: return .serverOffline
        }
    }
}

// MARK: - Anomalies

/// "Anomalies" group: recent latency / packet-loss anomalies in the range.
struct NetworkAnomaliesCard: View {
    let anomalies: [NetworkProbeAnomaly]

    @ScaledMetric(relativeTo: .body) private var tileSize: CGFloat = 30

    /// Cap the list to keep the section compact on mobile.
    private var visible: [NetworkProbeAnomaly] { Array(anomalies.prefix(20)) }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            GroupHeader(String(localized: "Anomalies"))
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(visible.enumerated()), id: \.element.id) { index, anomaly in
                    if index > 0 {
                        Divider()
                            .padding(.leading, 16 + tileSize + 12)
                    }
                    anomalyRow(anomaly)
                }
                if anomalies.count > visible.count {
                    Divider()
                    Text(String(localized: "+\(anomalies.count - visible.count) more"))
                        .font(.footnote)
                        .foregroundStyle(.tertiary)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 11)
                }
            }
            .cardSurface(padding: 0)
        }
    }

    private func anomalyRow(_ anomaly: NetworkProbeAnomaly) -> some View {
        HStack(spacing: 12) {
            Image(systemName: anomaly.isLatency ? "timer" : "wifi.slash")
                .font(.body.weight(.semibold))
                .foregroundStyle(Color.warningAmber)
                .frame(width: tileSize, height: tileSize)
                .background(
                    Color.warningAmber.opacity(0.16),
                    in: RoundedRectangle(cornerRadius: tileSize * 0.27, style: .continuous)
                )
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 1) {
                Text(anomalyDescription(anomaly))
                Text(verbatim: subtitle(anomaly))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
        .accessibilityElement(children: .combine)
    }

    private func subtitle(_ anomaly: NetworkProbeAnomaly) -> String {
        guard let date = anomaly.date else { return anomaly.targetName }
        let time = Calendar.current.isDateInToday(date)
            ? date.formatted(date: .omitted, time: .shortened)
            : date.formatted(.dateTime.month(.abbreviated).day().hour().minute())
        return "\(anomaly.targetName) · \(time)"
    }

    private func anomalyDescription(_ anomaly: NetworkProbeAnomaly) -> String {
        if anomaly.isLatency {
            return String(localized: "High latency \(NetworkFormat.latency(anomaly.value))")
        }
        return String(localized: "Packet loss \(NetworkFormat.loss(anomaly.value))")
    }
}
