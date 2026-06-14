import Foundation

enum CountryFlag {
    /// Convert an ISO 3166-1 alpha-2 country code to its flag emoji.
    /// Returns `nil` for missing or malformed codes.
    static func emoji(for countryCode: String?) -> String? {
        guard let code = countryCode?.trimmingCharacters(in: .whitespaces).uppercased(),
              code.count == 2,
              code.allSatisfy({ $0.isLetter })
        else {
            return nil
        }
        let base: UInt32 = 0x1F1E6 // Regional Indicator Symbol Letter A
        var scalarView = String.UnicodeScalarView()
        for ascii in code.unicodeScalars {
            guard let scalar = UnicodeScalar(base + ascii.value - 65) else { return nil }
            scalarView.append(scalar)
        }
        return String(scalarView)
    }
}
