import Foundation
import SwiftUI

enum Formatters {
    // Foundation formatter classes are documented thread-safe for read-only
    // use once configured; we cache them as statics to avoid the per-call
    // allocation cost (significant for chart rendering).
    // This mirrors the existing `ISO8601DateFormatter.shared` pattern in
    // `Utilities/Extensions.swift`.

    /// Shared formatter for human-readable byte counts (binary 1024 base).
    /// `ByteCountFormatter` is locale-aware: e.g. zh-Hans prefixes "字节"
    /// for under-1KB values.
    nonisolated(unsafe) private static let byteFormatter: ByteCountFormatter = {
        let f = ByteCountFormatter()
        f.countStyle = .binary
        f.allowedUnits = [.useBytes, .useKB, .useMB, .useGB, .useTB]
        // Render 0 as "0 bytes" rather than the locale word "Zero bytes".
        f.allowsNonnumericFormatting = false
        return f
    }()

    /// Cached HH:mm formatter for chart X-axis labels. Recreating
    /// `DateFormatter` on each Chart render hurts scroll performance.
    private static let chartTimeFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "HH:mm"
        return f
    }()

    /// Cached `RelativeDateTimeFormatter` for human-readable elapsed time
    /// (e.g. "5 minutes ago" / "5 分钟前"). Locale-aware.
    nonisolated(unsafe) private static let relativeFormatter: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .short
        return f
    }()

    static func formatBytes(_ bytes: Int64) -> String {
        byteFormatter.string(fromByteCount: bytes)
    }

    static func formatSpeed(_ bytesPerSec: Int64?) -> String {
        guard let bytesPerSec else {
            return "-"
        }
        return "\(byteFormatter.string(fromByteCount: bytesPerSec))/s"
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
        guard let value else {
            return "-"
        }
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
    static func formatRelativeTime(_ isoString: String) -> String {
        guard let date = ISO8601DateFormatter.shared.date(from: isoString) else {
            return isoString
        }
        return relativeFormatter.localizedString(for: date, relativeTo: Date())
    }
}
