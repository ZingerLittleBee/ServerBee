import Foundation
import SwiftUI

/// Port of `apps/mobile/src/lib/format.ts` — all display formatters used across
/// server views for bytes, speeds, uptime, percentages, and relative times.
enum Formatters {
    // MARK: - Bytes

    /// Format a byte count into a human-readable string (B, KB, MB, GB, TB).
    static func formatBytes(_ bytes: Int64) -> String {
        if bytes < 1024 {
            return "\(bytes) B"
        }
        if bytes < 1_048_576 {
            return String(format: "%.1f KB", Double(bytes) / 1024)
        }
        if bytes < 1_073_741_824 {
            return String(format: "%.1f MB", Double(bytes) / 1_048_576)
        }
        if bytes < 1_099_511_627_776 {
            return String(format: "%.1f GB", Double(bytes) / 1_073_741_824)
        }
        return String(format: "%.1f TB", Double(bytes) / 1_099_511_627_776)
    }

    /// Format a byte count into a compact string without unit suffix, suitable for chart labels.
    static func formatBytesCompact(_ bytes: Int64) -> String {
        if bytes < 1024 {
            return "\(bytes)B"
        }
        if bytes < 1_048_576 {
            return String(format: "%.0fK", Double(bytes) / 1024)
        }
        if bytes < 1_073_741_824 {
            return String(format: "%.0fM", Double(bytes) / 1_048_576)
        }
        return String(format: "%.1fG", Double(bytes) / 1_073_741_824)
    }

    // MARK: - Network Speed

    /// Format bytes-per-second into a readable speed string.
    static func formatSpeed(_ bytesPerSec: Int64?) -> String {
        guard let bytesPerSec else {
            return "-"
        }
        if bytesPerSec < 1024 {
            return "\(bytesPerSec) B/s"
        }
        if bytesPerSec < 1_048_576 {
            return String(format: "%.1f KB/s", Double(bytesPerSec) / 1024)
        }
        return String(format: "%.1f MB/s", Double(bytesPerSec) / 1_048_576)
    }

    // MARK: - Uptime

    /// Format seconds into a human-readable uptime string (e.g. "3d 12h" or "2h 30m").
    static func formatUptime(_ seconds: Int64) -> String {
        let d = seconds / 86_400
        let h = (seconds % 86_400) / 3600
        if d > 0 {
            return "\(d)d \(h)h"
        }
        let m = (seconds % 3600) / 60
        if h > 0 {
            return "\(h)h \(m)m"
        }
        return "\(m)m"
    }

    // MARK: - Percentage

    /// Format a Double (0-100) as a percentage string.
    static func formatPercentage(_ value: Double?) -> String {
        guard let value else {
            return "-"
        }
        return String(format: "%.1f%%", value)
    }

    // MARK: - Memory / Disk Ratio

    /// Format a used/total byte pair as "X.X GB / Y.Y GB".
    static func formatBytesRatio(used: Int64?, total: Int64?) -> String {
        guard let used, let total else { return "-" }
        return "\(formatBytes(used)) / \(formatBytes(total))"
    }

    // MARK: - Relative Time

    /// Format an ISO 8601 date string into a relative time string (e.g. "5m ago").
    static func formatRelativeTime(_ isoString: String) -> String {
        guard let date = ISO8601DateFormatter.shared.date(from: isoString) else {
            return isoString
        }

        let now = Date()
        let interval = now.timeIntervalSince(date)

        if interval < 60 {
            return String(localized: "just now")
        }
        if interval < 3600 {
            let minutes = Int(interval / 60)
            return "\(minutes)m ago"
        }
        if interval < 86_400 {
            let hours = Int(interval / 3600)
            return "\(hours)h ago"
        }
        let days = Int(interval / 86_400)
        return "\(days)d ago"
    }

    // MARK: - CPU Usage Color

    /// Returns a color indicating CPU usage severity.
    static func cpuColor(for usage: Double) -> Color {
        if usage >= 90 { return .red }
        if usage >= 70 { return .orange }
        if usage >= 50 { return .yellow }
        return .green
    }

    /// Returns a color indicating disk/memory usage severity.
    static func usageColor(for percentage: Double) -> Color {
        if percentage >= 90 { return .red }
        if percentage >= 75 { return .orange }
        if percentage >= 50 { return .yellow }
        return .green
    }

    // MARK: - Short Time Formatting for Charts

    private static let chartTimeFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm"
        return formatter
    }()

    private static let chartDateTimeFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "MM/dd HH:mm"
        return formatter
    }()

    /// Format a Date into a short time string for chart axis labels.
    static func formatChartTime(_ date: Date) -> String {
        chartTimeFormatter.string(from: date)
    }

    /// Format a Date into a short date+time string for chart axis labels.
    static func formatChartDateTime(_ date: Date) -> String {
        chartDateTimeFormatter.string(from: date)
    }
}
