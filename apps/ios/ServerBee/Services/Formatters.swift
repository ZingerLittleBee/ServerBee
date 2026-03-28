import Foundation

enum Formatters {
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
}
