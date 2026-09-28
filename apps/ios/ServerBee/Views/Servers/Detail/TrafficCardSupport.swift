import Foundation
import SwiftUI

// Shared helpers for the server Traffic tab cards (`TrafficCards.swift`,
// `CostInsightsCard.swift`).

/// Label-over-value figure used in the traffic and cost cards' column grids
/// ("Download 318 GB", "Per day $0.40").
struct TrafficStatCell: View {
    let label: String
    let value: String
    var systemImage: String?
    var iconColor: Color = .secondary
    var valueFont: Font = .title3.bold()

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 4) {
                if let systemImage {
                    Image(systemName: systemImage)
                        .font(.caption2.weight(.bold))
                        .foregroundStyle(iconColor)
                        .accessibilityHidden(true)
                }
                Text(label)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Text(value)
                .font(valueFont)
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.7)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}

/// Formats the server's UTC `"yyyy-MM-dd"` day strings for display. The
/// styles are pinned to GMT so the day never shifts with the device zone.
enum TrafficDayFormat {
    private static let shortStyle = Date.FormatStyle(timeZone: .gmt).month(.abbreviated).day()
    private static let fullStyle = Date.FormatStyle(date: .abbreviated, time: .omitted, timeZone: .gmt)

    /// Gregorian calendar in GMT, so charts bin `unit: .day` marks on the
    /// same day boundary the server accounts in.
    static let utcCalendar: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = .gmt
        return calendar
    }()

    /// "Sep 17" for a parsed day.
    static func short(_ date: Date) -> String {
        date.formatted(shortStyle)
    }

    /// "Sep 17" for a raw day string; falls back to the raw value.
    static func short(day: String) -> String {
        Formatters.parseDay(day).map(short) ?? day
    }

    /// "Sep 17, 2026" for a raw day string; falls back to the raw value.
    static func full(day: String) -> String {
        Formatters.parseDay(day).map { $0.formatted(fullStyle) } ?? day
    }
}

/// Y-axis ticks in binary byte units (0, step, 2·step) so labels read as
/// round values ("20 GB") under the 1024-based byte formatter.
struct TrafficAxisScale {
    let ticks: [Double]
    let upperBound: Double

    init(maxBytes: Int64) {
        guard maxBytes > 0 else {
            ticks = [0]
            upperBound = 1
            return
        }
        let value = Double(maxBytes)
        var unit: Double = 1
        while unit * 1024 <= value, unit < pow(1024, 4) {
            unit *= 1024
        }
        let raw = value / unit / 2
        let magnitude = pow(10, floor(log10(raw)))
        let multiplier = [1, 2, 2.5, 5, 10].first { $0 * magnitude >= raw } ?? 10
        // Never step below one byte: sub-byte ticks would all print "0 bytes".
        let step = max(1, multiplier * magnitude * unit)
        ticks = [0, step, step * 2]
        upperBound = step * 2
    }
}
