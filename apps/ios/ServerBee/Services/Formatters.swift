import Foundation
import SwiftUI

enum Formatters {
    // Foundation formatter classes are documented thread-safe for read-only
    // use once configured; we cache them as statics to avoid the per-call
    // allocation cost (significant for chart rendering).
    // This mirrors the existing `ISO8601DateFormatter.shared` pattern in
    // `Utilities/Extensions.swift`.

    /// Binary (1024) byte units shared by every byte and rate readout.
    static let byteUnits = ["B", "KB", "MB", "GB", "TB", "PB"]

    /// Cached HH:mm formatter for chart X-axis labels. Recreating
    /// `DateFormatter` on each Chart render hurts scroll performance.
    private static let chartTimeFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "HH:mm"
        return f
    }()

    /// Parses the server's `"yyyy-MM-dd"` calendar-day strings (traffic/uptime).
    /// Fixed to UTC so the day boundary matches the server's accounting.
    private static let dayParser: DateFormatter = {
        let f = DateFormatter()
        f.calendar = Calendar(identifier: .gregorian)
        f.locale = Locale(identifier: "en_US_POSIX")
        f.timeZone = TimeZone(secondsFromGMT: 0)
        f.dateFormat = "yyyy-MM-dd"
        return f
    }()

    /// Short "M/d" label for daily-bucket chart axes.
    private static let dayAxisFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "M/d"
        return f
    }()

    /// Cached `RelativeDateTimeFormatter` for human-readable elapsed time
    /// (e.g. "5 minutes ago" / "5 分钟前"). Locale-aware.
    nonisolated(unsafe) private static let relativeFormatter: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .short
        return f
    }()

    /// "512 B", "1.5 MB", "20 GB": binary units, at most one decimal, no locale words.
    static func formatBytes(_ bytes: Int64) -> String {
        let value = Double(max(bytes, 0))
        let index = byteUnitIndex(for: value)
        return "\(byteNumber(value, unitIndex: index)) \(byteUnits[index])"
    }

    static func formatSpeed(_ bytesPerSec: Int64?) -> String {
        guard let bytesPerSec else { return "—" }
        return "\(formatBytes(bytesPerSec))/s"
    }

    /// Index into `byteUnits` for a byte count. Rolls over at 1000 rather than
    /// 1024 so a readout never needs four digits ("1 MB", not "1010 KB").
    static func byteUnitIndex(for bytes: Double) -> Int {
        var index = 0
        var scaled = bytes
        while scaled >= 1000, index < byteUnits.count - 1 {
            scaled /= 1024
            index += 1
        }
        return index
    }

    /// `bytes` expressed in `byteUnits[unitIndex]`: whole bytes, and one decimal
    /// below 100 of a larger unit, trailing zero dropped ("4.8", "12", "256").
    static func byteNumber(_ bytes: Double, unitIndex: Int) -> String {
        let scaled = bytes / pow(1024, Double(unitIndex))
        let digits = unitIndex == 0 || scaled >= 100 ? 0 : 1
        return scaled.formatted(.number.precision(.fractionLength(0 ... digits)).grouping(.never))
    }

    static func formatUptime(_ seconds: Int64) -> String {
        let d = seconds / 86_400
        let h = (seconds % 86_400) / 3600
        if d > 0 {
            return "\(d)d \(h)h"
        }
        let m = (seconds % 3600) / 60
        return "\(h)h \(m)m"
    }

    static func formatPercentage(_ value: Double?) -> String {
        guard let value else { return "—" }
        return String(format: "%.1f%%", value)
    }

    static func formatBytesRatio(used: Int64?, total: Int64?) -> String? {
        guard let used, let total else { return nil }
        return "\(formatBytes(used)) / \(formatBytes(total))"
    }

    /// Returns a colour representing CPU load severity.
    static func cpuColor(for value: Double) -> Color {
        switch value {
        case ..<50: return .cpuColor
        case ..<80: return .orange
        default: return .red
        }
    }

    /// Returns a colour representing generic usage severity (memory, disk).
    static func usageColor(for value: Double) -> Color {
        switch value {
        case ..<50: return .green
        case ..<80: return .orange
        default: return .red
        }
    }

    /// Short time label for chart X-axis.
    static func formatChartTime(_ date: Date) -> String {
        chartTimeFormatter.string(from: date)
    }

    /// Locale-aware relative time, e.g. "5 minutes ago" / "5 分钟前".
    /// Returns the original ISO string if parsing fails.
    ///
    /// Most timestamps describe something that already happened; for those a
    /// time at or slightly ahead of the device clock (server clock skew) reads
    /// "just now" rather than "in 0 sec.". Pass `allowsFuture` for times that
    /// can legitimately lie ahead (next run, expiry, maintenance windows).
    static func formatRelativeTime(_ isoString: String, allowsFuture: Bool = false, now: Date = Date()) -> String {
        guard let date = ISO8601DateFormatter.shared.date(from: isoString) else {
            return isoString
        }
        if !allowsFuture, now.timeIntervalSince(date) < 5 {
            return String(localized: "just now")
        }
        return relativeFormatter.localizedString(for: date, relativeTo: now)
    }

    /// Parse a server `"yyyy-MM-dd"` day string into a UTC `Date`.
    static func parseDay(_ string: String) -> Date? {
        dayParser.date(from: string)
    }

    /// Short "M/d" label for a daily-bucket date.
    static func formatDayAxis(_ date: Date) -> String {
        dayAxisFormatter.string(from: date)
    }

    /// Currency amount, e.g. "$12.50". Falls back to a plain number + code for
    /// non-ISO currency strings.
    static func formatCurrency(_ amount: Double?, code: String) -> String {
        guard let amount else { return "—" }
        let f = NumberFormatter()
        f.numberStyle = .currency
        f.currencyCode = code
        f.maximumFractionDigits = amount < 1 ? 3 : 2
        if let s = f.string(from: NSNumber(value: amount)) { return s }
        return String(format: "%.2f %@", amount, code)
    }

    /// Fine-grained rate (e.g. cost-per-second) with more precision for tiny
    /// values so they don't collapse to "$0.00".
    static func formatCurrencyRate(_ amount: Double?, code: String) -> String {
        guard let amount else { return "—" }
        let f = NumberFormatter()
        f.numberStyle = .currency
        f.currencyCode = code
        f.maximumFractionDigits = amount < 0.01 ? 6 : (amount < 1 ? 4 : 2)
        if let s = f.string(from: NSNumber(value: amount)) { return s }
        return String(format: "%.4f %@", amount, code)
    }
}
