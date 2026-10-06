import Foundation

/// Converts the date picker's instant to/from the server's literal local date.
/// This never calculates a renewal boundary or advances a schedule.
enum BillingDate {
    static func string(from date: Date, timezone: String) -> String? {
        guard let formatter = formatter(timezone: timezone) else { return nil }
        return formatter.string(from: date)
    }

    static func date(from string: String, timezone: String) -> Date? {
        guard let formatter = formatter(timezone: timezone) else { return nil }
        // Parse at noon directly: some valid local dates have no midnight at DST start.
        formatter.dateFormat = "yyyy-MM-dd HH:mm"
        let noon = "\(string) 12:00"
        guard let date = formatter.date(from: noon), formatter.string(from: date) == noon else { return nil }
        return date
    }

    static func display(from date: Date, timezone: String) -> String {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.timeZone = TimeZone(identifier: timezone) ?? .gmt
        formatter.dateStyle = .medium
        formatter.timeStyle = .none
        return formatter.string(from: date)
    }

    private static func formatter(timezone: String) -> DateFormatter? {
        guard let zone = TimeZone(identifier: timezone) else { return nil }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = zone
        formatter.calendar = calendar
        formatter.timeZone = zone
        formatter.dateFormat = "yyyy-MM-dd"
        formatter.isLenient = false
        return formatter
    }
}
