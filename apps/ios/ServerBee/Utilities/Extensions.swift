import SwiftUI

// MARK: - Color Extensions

extension Color {
    static let serverOnline = Color(red: 0x22 / 255.0, green: 0xC5 / 255.0, blue: 0x5E / 255.0)
    static let serverOffline = Color(red: 0xEF / 255.0, green: 0x44 / 255.0, blue: 0x44 / 255.0)
    static let alertFiring = Color(red: 0xF9 / 255.0, green: 0x73 / 255.0, blue: 0x16 / 255.0)
    static let alertResolved = Color.serverOnline
    static let cpuColor = Color(red: 0x38 / 255.0, green: 0xBD / 255.0, blue: 0xF8 / 255.0)
    static let memoryColor = Color(red: 0xA7 / 255.0, green: 0x8B / 255.0, blue: 0xFA / 255.0)
    static let diskColor = Color(red: 0xFB / 255.0, green: 0xBD / 255.0, blue: 0x23 / 255.0)
    static let networkColor = Color(red: 0x34 / 255.0, green: 0xD3 / 255.0, blue: 0x99 / 255.0)
}

// MARK: - ISO8601DateFormatter Extension

/// Tolerant ISO 8601 parser that handles backend timestamps with OR without
/// fractional seconds. The Rust backend uses `chrono::DateTime::to_rfc3339()`,
/// which only emits fractional seconds when the source value has subsecond
/// precision — so both forms appear in real payloads.
final class TolerantISO8601Parser: @unchecked Sendable {
    private let withFractional: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()

    private let withoutFractional: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()

    func date(from string: String) -> Date? {
        withFractional.date(from: string) ?? withoutFractional.date(from: string)
    }

    func string(from date: Date) -> String {
        withFractional.string(from: date)
    }
}

extension ISO8601DateFormatter {
    /// Tolerant shared parser. Use `.date(from:)` / `.string(from:)` on it.
    /// (Note: this is now a wrapper type with the same method shape, not an
    /// actual `ISO8601DateFormatter` instance.)
    nonisolated(unsafe) static let shared: TolerantISO8601Parser = TolerantISO8601Parser()
}
