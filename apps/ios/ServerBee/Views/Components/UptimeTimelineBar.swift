import SwiftUI

/// A compact, tappable uptime timeline rendered as a row of day segments.
///
/// Each day is colour-coded by health (operational / degraded / down / no
/// data). Tapping a segment reports it back via `onSelect` so the host can show
/// the date, ratio and incident count. Segments size themselves to the
/// available width so the full window always fits on one line.
struct UptimeTimelineBar: View {
    let days: [UptimeDailyEntry]
    var selectedDate: String?
    var onSelect: (UptimeDailyEntry) -> Void = { _ in }

    private let segmentSpacing: CGFloat = 1.5
    private let barHeight: CGFloat = 34

    var body: some View {
        GeometryReader { geo in
            let count = max(days.count, 1)
            let totalSpacing = segmentSpacing * CGFloat(count - 1)
            let segWidth = max(2, (geo.size.width - totalSpacing) / CGFloat(count))
            HStack(spacing: segmentSpacing) {
                ForEach(days) { day in
                    RoundedRectangle(cornerRadius: 1.5)
                        .fill(color(for: day.status))
                        .frame(width: segWidth, height: barHeight)
                        .opacity(selectedDate == nil || selectedDate == day.date ? 1 : 0.45)
                        .contentShape(Rectangle())
                        .onTapGesture { onSelect(day) }
                        .accessibilityLabel(Text(day.date))
                        .accessibilityValue(Text(accessibilityValue(for: day)))
                }
            }
        }
        .frame(height: barHeight)
    }

    private func color(for status: UptimeStatus) -> Color {
        switch status {
        case .operational: .serverOnline
        case .degraded: .warningAmber
        case .down: .serverOffline
        case .noData: Color(.systemGray4)
        }
    }

    private func accessibilityValue(for day: UptimeDailyEntry) -> String {
        guard let ratio = day.ratio else { return String(localized: "No data") }
        return String(format: "%.1f%%", ratio * 100)
    }
}

/// Legend explaining the timeline colours.
struct UptimeLegend: View {
    var body: some View {
        HStack(spacing: 14) {
            item(.serverOnline, String(localized: "Operational"))
            item(.warningAmber, String(localized: "Degraded"))
            item(.serverOffline, String(localized: "Down"))
            item(Color(.systemGray4), String(localized: "No data"))
        }
        .font(.caption2)
        .foregroundStyle(.secondary)
    }

    private func item(_ color: Color, _ label: String) -> some View {
        HStack(spacing: 4) {
            RoundedRectangle(cornerRadius: 2)
                .fill(color)
                .frame(width: 9, height: 9)
            Text(label)
        }
    }
}
