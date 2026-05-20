import SwiftUI

// MARK: - Color Extensions

extension Color {
    static let serverOnline = Color("ServerOnline")
    static let serverOffline = Color("ServerOffline")
    static let alertFiring = Color("AlertFiring")
    static let alertResolved = Color("ServerOnline")
    static let warningAmber = Color("WarningAmber")
    static let brandAccent = Color("BrandAccent")
    static let cpuColor = Color("CPUColor")
    static let memoryColor = Color("MemoryColor")
    static let diskColor = Color("DiskColor")
    static let networkColor = Color("NetworkColor")
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
    static let shared: TolerantISO8601Parser = TolerantISO8601Parser()
}
