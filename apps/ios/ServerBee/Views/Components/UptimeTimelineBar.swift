import SwiftUI

/// A compact, tappable uptime timeline rendered as a row of day segments.
///
/// Each day is colour-coded by health (operational / degraded / down / no
/// data). Tapping a segment reports it back via `onSelect` so the host can show
/// the date, ratio and incident count. Segments size themselves to the
/// available width so the full window always fits on one line; the gap between
/// them tightens for long (e.g. 90-day) windows.
struct UptimeTimelineBar: View {
    let days: [UptimeDailyEntry]
    var selectedDate: String?
    var onSelect: (UptimeDailyEntry) -> Void

    @ScaledMetric(relativeTo: .caption) private var barHeight: CGFloat = 26

    init(
        days: [UptimeDailyEntry],
        selectedDate: String? = nil,
        onSelect: @escaping (UptimeDailyEntry) -> Void = { _ in }
    ) {
        self.days = days
        self.selectedDate = selectedDate
        self.onSelect = onSelect
    }

    var body: some View {
        GeometryReader { geo in
            let count = max(days.count, 1)
            let spacing = segmentSpacing(count: count, width: geo.size.width)
            let totalSpacing = spacing * CGFloat(count - 1)
            let segWidth = max(2, (geo.size.width - totalSpacing) / CGFloat(count))
            HStack(spacing: spacing) {
                ForEach(days) { day in
                    RoundedRectangle(cornerRadius: min(2, segWidth / 2), style: .continuous)
                        .fill(color(for: day.status))
                        .frame(width: segWidth, height: barHeight)
                        .opacity(selectedDate == nil || selectedDate == day.date ? 1 : 0.45)
                        .contentShape(Rectangle())
                        .onTapGesture { onSelect(day) }
                        .accessibilityLabel(Text(day.date))
                        .accessibilityValue(Text(accessibilityValue(for: day)))
                        .accessibilityAddTraits(.isButton)
                }
            }
        }
        .frame(height: barHeight)
    }

    /// 3pt gaps while every segment stays at least twice as wide as the gap (a
    /// 30-day window on a phone card, ~8pt bars); dense windows such as 90 days
    /// fall back to hairline 1.5pt gaps.
    private func segmentSpacing(count: Int, width: CGFloat) -> CGFloat {
        let roomy: CGFloat = 3
        let widthAtRoomy = (width - roomy * CGFloat(count - 1)) / CGFloat(count)
        return widthAtRoomy >= roomy * 2 ? roomy : 1.5
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
    @ScaledMetric(relativeTo: .caption2) private var swatch: CGFloat = 9

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
            RoundedRectangle(cornerRadius: 2, style: .continuous)
                .fill(color)
                .frame(width: swatch, height: swatch)
                .accessibilityHidden(true)
            Text(label)
        }
    }
}
