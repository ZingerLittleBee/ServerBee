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

    /// Caption for a run of daily rows: "last 30 days" while the history
    /// reaches yesterday or today, otherwise "through Sep 3" (it stopped, e.g.
    /// an offline server). Days without traffic have no row, so the span
    /// counts calendar days rather than rows.
    static func rangeCaption(days: [String], now: Date = Date()) -> String? {
        let dates = days.compactMap(Formatters.parseDay)
        guard let first = dates.min(), let last = dates.max() else { return nil }
        let today = utcCalendar.startOfDay(for: now)
        if (utcCalendar.dateComponents([.day], from: last, to: today).day ?? 0) > 1 {
            return String(localized: "through \(short(last))")
        }
        let span = (utcCalendar.dateComponents([.day], from: first, to: last).day ?? 0) + 1
        return span == 1 ? String(localized: "last 1 day") : String(localized: "last \(span) days")
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

/// Y-axis ticks in one binary byte unit (0, step, 2·step), with the top tick
/// kept under 1000 of that unit so labels read "0.5 GB · 1 GB", not "1,000 MB".
struct TrafficAxisScale {
    let ticks: [Double]
    let upperBound: Double
    /// Index into `Formatters.byteUnits` shared by every tick label.
    let unitIndex: Int

    init(maxBytes: Int64) {
        guard maxBytes > 0 else {
            ticks = [0]
            upperBound = 1
            unitIndex = 0
            return
        }
        let value = Double(maxBytes)
        var index = Formatters.byteUnitIndex(for: value)
        var step = Self.step(for: value, unitIndex: index)
        if step * 2 >= 1000 * pow(1024, Double(index)), index < Formatters.byteUnits.count - 1 {
            index += 1
            step = Self.step(for: value, unitIndex: index)
        }
        ticks = [0, step, step * 2]
        upperBound = step * 2
        unitIndex = index
    }

    /// "0", "0.25 GB", "500 MB": every tick in the same unit.
    func label(for bytes: Double) -> String {
        guard bytes > 0 else { return "0" }
        let scaled = bytes / pow(1024, Double(unitIndex))
        let number = scaled.formatted(.number.precision(.fractionLength(0 ... 2)).grouping(.never))
        return "\(number) \(Formatters.byteUnits[unitIndex])"
    }

    /// A 1 / 2 / 2.5 / 5 × 10ⁿ step in `unitIndex` units covering half of `value`.
    private static func step(for value: Double, unitIndex: Int) -> Double {
        let unit = pow(1024, Double(unitIndex))
        let raw = value / unit / 2
        let magnitude = pow(10, floor(log10(raw)))
        let multiplier = [1, 2, 2.5, 5, 10].first { $0 * magnitude >= raw } ?? 10
        // Never step below one byte: sub-byte ticks would all print "0 B".
        return max(1, multiplier * magnitude * unit)
    }
}
