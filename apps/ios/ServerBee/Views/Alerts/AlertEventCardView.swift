import SwiftUI

/// One alert event row in the inset-grouped alerts list: status + trigger count
/// + compact age on the first line, then the rule name and server name.
struct AlertEventCardView: View {
    let event: MobileAlertEvent

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(spacing: 8) {
                AlertStatusBadge(status: event.status)
                if event.count > 1 {
                    AlertTagCapsule.count(event.count)
                }
                Spacer(minLength: 8)
                Text(CompactAlertTime.string(from: event.eventAt))
                    .font(.footnote)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Text(event.ruleName)
                .font(.headline)
                .foregroundStyle(.primary)

            Text(event.serverName)
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(accessibilityLabelText))
    }

    private var accessibilityLabelText: String {
        let status = event.status == .firing
            ? String(localized: "Firing")
            : String(localized: "Resolved")
        let relative = Formatters.formatRelativeTime(event.eventAt)
        var parts = [status, event.ruleName, event.serverName, relative]
        if event.count > 1 {
            parts.append(String(format: String(localized: "Triggered %d times"), event.count))
        }
        return parts.joined(separator: ", ")
    }
}

/// Mail-style compact age for list rows: "12m" / "2h" within the last day,
/// "Yesterday", then a short date. All pieces come from system formatters, so
/// they localize without catalog entries.
enum CompactAlertTime {
    /// Relative-day formatter ("Yesterday" / "昨天"), date only.
    private static let relativeDay: DateFormatter = {
        let f = DateFormatter()
        f.dateStyle = .medium
        f.timeStyle = .none
        f.doesRelativeDateFormatting = true
        return f
    }()

    static func string(from isoString: String, now: Date = Date()) -> String {
        guard let date = ISO8601DateFormatter.shared.date(from: isoString) else {
            return isoString
        }
        let elapsed = now.timeIntervalSince(date)
        if elapsed < 3600 {
            return Duration.seconds(max(elapsed, 60))
                .formatted(.units(allowed: [.minutes], width: .narrow, fractionalPart: .hide(rounded: .down)))
        }
        if elapsed < 86_400 {
            return Duration.seconds(elapsed)
                .formatted(.units(allowed: [.hours], width: .narrow, fractionalPart: .hide(rounded: .down)))
        }
        let calendar = Calendar.current
        if calendar.isDateInYesterday(date) {
            return relativeDay.string(from: date)
        }
        if calendar.isDate(date, equalTo: now, toGranularity: .year) {
            return date.formatted(.dateTime.month(.abbreviated).day())
        }
        return date.formatted(.dateTime.year().month(.abbreviated).day())
    }
}
