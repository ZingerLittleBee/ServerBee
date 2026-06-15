import Charts
import SwiftUI

// MARK: - Summary

/// Probe summary: online state, last probe time, 24h anomaly count.
struct NetworkSummaryCard: View {
    let summary: NetworkProbeServerSummary

    var body: some View {
        SectionCard {
            HStack(alignment: .center) {
                VStack(alignment: .leading, spacing: 4) {
                    StatusPill(isOnline: summary.online)
                    if let last = summary.lastProbeAt {
                        Text(String(localized: "Last probe \(Formatters.formatRelativeTime(last))"))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                Spacer()
                if summary.anomalyCount > 0 {
                    Chip(
                        text: String(localized: "\(summary.anomalyCount) anomalies (24h)"),
                        systemImage: "exclamationmark.triangle.fill",
                        color: .warningAmber
                    )
                } else {
                    Chip(text: String(localized: "Healthy"), systemImage: "checkmark.circle.fill", color: .serverOnline)
                }
            }
        }
    }
}

// MARK: - Latency Chart

/// Average latency over time, one line per target.
struct NetworkLatencyChart: View {
    let records: [ProbeRecordDto]
    let targets: [NetworkProbeTarget]

    private struct Point: Identifiable {
        let id: String
        let date: Date
        let latency: Double
        let target: String
    }

    private var nameByID: [String: String] {
        Dictionary(targets.map { ($0.id, $0.name) }, uniquingKeysWith: { a, _ in a })
    }

    private var points: [Point] {
        records.compactMap { rec in
            guard let date = rec.date, let latency = rec.avgLatency else { return nil }
            let name = nameByID[rec.targetId] ?? rec.targetId
            return Point(id: "\(rec.targetId)-\(rec.timestamp)", date: date, latency: latency, target: name)
        }
    }

    var body: some View {
        ChartSection(title: String(localized: "Latency")) {
            if points.isEmpty {
                emptyChart
            } else {
                Chart(points) { point in
                    LineMark(
                        x: .value("Time", point.date),
                        y: .value("Latency", point.latency),
                        series: .value("Target", point.target)
                    )
                    .foregroundStyle(by: .value("Target", point.target))
                    .interpolationMethod(.catmullRom)
                }
                .chartLegend(position: .bottom, alignment: .leading)
                .chartYAxis {
                    AxisMarks { value in
                        AxisGridLine()
                        AxisValueLabel {
                            if let ms = value.as(Double.self) {
                                Text("\(Int(ms)) ms")
                            }
                        }
                    }
                }
                .timeXAxis()
            }
        }
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

/// Per-provider grouped target health cards.
struct NetworkTargetsCard: View {
    let targets: [NetworkProbeTarget]
    let summaries: [TargetSummary]

    private var summaryByID: [String: TargetSummary] {
        Dictionary(summaries.map { ($0.targetId, $0) }, uniquingKeysWith: { a, _ in a })
    }

    /// Group targets by provider, ordered ct/cu/cm/international/custom.
    private var groups: [(provider: String, targets: [NetworkProbeTarget])] {
        let grouped = Dictionary(grouping: targets, by: { $0.provider })
        return grouped
            .map { (provider: $0.key, targets: $0.value.sorted { $0.name < $1.name }) }
            .sorted { NetworkProvider.order(for: $0.provider) < NetworkProvider.order(for: $1.provider) }
    }

    var body: some View {
        SectionCard(String(localized: "Targets"), systemImage: "scope") {
            if targets.isEmpty {
                Text(String(localized: "No probe targets assigned"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            } else {
                VStack(alignment: .leading, spacing: 16) {
                    ForEach(groups, id: \.provider) { group in
                        VStack(alignment: .leading, spacing: 8) {
                            Text(NetworkProvider.label(for: group.provider))
                                .font(.caption.bold())
                                .foregroundStyle(.secondary)
                            ForEach(group.targets) { target in
                                targetRow(target)
                            }
                        }
                    }
                }
            }
        }
    }

    private func targetRow(_ target: NetworkProbeTarget) -> some View {
        let summary = summaryByID[target.id]
        return VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(target.name)
                    .font(.subheadline.weight(.medium))
                Chip(text: target.probeType.uppercased(), color: .secondary)
                Spacer()
                Text(NetworkFormat.latency(summary?.avgLatency))
                    .font(.subheadline.bold().monospacedDigit())
                    .foregroundStyle(latencyColor(summary?.avgLatency))
            }
            HStack(spacing: 8) {
                Text(target.target)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer()
                if let loss = summary?.packetLoss, loss > 0 {
                    Text(String(localized: "loss \(NetworkFormat.loss(loss))"))
                        .font(.caption)
                        .foregroundStyle(lossColor(loss))
                }
            }
        }
        .padding(.vertical, 4)
    }

    private func latencyColor(_ ms: Double?) -> Color {
        guard let ms else { return .secondary }
        switch ms {
        case ..<100: return .serverOnline
        case ..<300: return .warningAmber
        default: return .serverOffline
        }
    }

    private func lossColor(_ ratio: Double) -> Color {
        switch ratio {
        case ..<0.1: return .warningAmber
        default: return .serverOffline
        }
    }
}

// MARK: - Anomalies

/// Recent probe anomalies list.
struct NetworkAnomaliesCard: View {
    let anomalies: [NetworkProbeAnomaly]

    /// Cap the list to keep the section compact on mobile.
    private var visible: [NetworkProbeAnomaly] { Array(anomalies.prefix(20)) }

    var body: some View {
        SectionCard(String(localized: "Anomalies"), systemImage: "exclamationmark.triangle") {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(visible) { anomaly in
                    HStack(alignment: .firstTextBaseline, spacing: 10) {
                        Image(systemName: anomaly.isLatency ? "timer" : "wifi.slash")
                            .font(.caption)
                            .foregroundStyle(Color.warningAmber)
                            .frame(width: 18)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(anomaly.targetName)
                                .font(.subheadline)
                            Text(anomalyDescription(anomaly))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        if let date = anomaly.date {
                            Text(date, style: .time)
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                        }
                    }
                }
                if anomalies.count > visible.count {
                    Text(String(localized: "+\(anomalies.count - visible.count) more"))
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
            }
        }
    }

    private func anomalyDescription(_ anomaly: NetworkProbeAnomaly) -> String {
        if anomaly.isLatency {
            return String(localized: "High latency \(NetworkFormat.latency(anomaly.value))")
        }
        return String(localized: "Packet loss \(NetworkFormat.loss(anomaly.value))")
    }
}
