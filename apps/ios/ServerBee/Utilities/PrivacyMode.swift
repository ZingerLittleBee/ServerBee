import Foundation
import SwiftUI

/// Privacy mode hides the last two parts of every IP address shown in the app,
/// so screens can be shared or screenshotted without revealing hosts. It only
/// changes what is displayed: requests, edit forms and copy-to-block actions
/// keep the real address.
enum PrivacyMode {
    /// `@AppStorage` key for the Settings toggle.
    static let storageKey = "privacyMode"
}

extension EnvironmentValues {
    /// Whether IP addresses should be masked. Injected once at the root from
    /// the Settings toggle, so every screen (sheets included) follows it live.
    @Entry var privacyMode = false
}

extension String {
    /// This string with IP addresses masked when `enabled` (see `IPMask`).
    func maskingIPs(_ enabled: Bool) -> String {
        enabled ? IPMask.mask(self) : self
    }
}

/// Masks the last two parts of IP addresses found anywhere in a string:
/// IPv4 `203.0.113.7` → `203.0.*.*`, IPv6 `2001:db8:1:2:3:4:5:6` →
/// `2001:db8:1:2:3:4:*:*`. Surrounding text (hostnames, ports, CIDR prefix
/// lengths, URL paths) is left untouched.
enum IPMask {
    // Not part of a longer dotted/word token, so version strings such as
    // "1.2.3.4.5" and hostnames such as "a1.2.3.4" are skipped.
    private static let ipv4 = try? NSRegularExpression(
        pattern: #"(?<![\w.])(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})(?!\w|\.\d)"#
    )
    // Candidate runs with at least two colons; `inet_pton` decides whether a
    // run is really an address (times and MAC addresses are rejected). A run
    // followed by "." is the prefix of an IPv4-mapped address, left to `ipv4`.
    private static let ipv6 = try? NSRegularExpression(
        pattern: #"(?<![\w:.])(?=[0-9A-Fa-f]*:[0-9A-Fa-f]*:)[0-9A-Fa-f:]+(%[\w.]+)?(?![\w:.])"#
    )

    static func mask(_ text: String) -> String {
        guard text.contains(where: { $0 == "." || $0 == ":" }) else { return text }
        return replace(ipv6, in: replace(ipv4, in: text, with: maskIPv4), with: maskIPv6)
    }

    private static func replace(
        _ regex: NSRegularExpression?,
        in text: String,
        with transform: (String) -> String?
    ) -> String {
        guard let regex else { return text }
        let ns = text as NSString
        var result = ""
        var cursor = 0
        for match in regex.matches(in: text, range: NSRange(location: 0, length: ns.length)) {
            let found = ns.substring(with: match.range)
            guard let masked = transform(found) else { continue }
            result += ns.substring(with: NSRange(location: cursor, length: match.range.location - cursor))
            result += masked
            cursor = match.range.location + match.range.length
        }
        return result + ns.substring(from: cursor)
    }

    private static func maskIPv4(_ address: String) -> String? {
        let octets = address.split(separator: ".")
        guard octets.count == 4, octets.allSatisfy({ (Int($0) ?? 256) <= 255 }) else { return nil }
        return "\(octets[0]).\(octets[1]).*.*"
    }

    private static func maskIPv6(_ candidate: String) -> String? {
        let parts = candidate.split(separator: "%", maxSplits: 1)
        let address = String(parts[0])
        var bytes = in6_addr()
        guard inet_pton(AF_INET6, address, &bytes) == 1 else { return nil }
        let groups = withUnsafeBytes(of: &bytes) { raw in
            (0..<8).map { UInt16(raw[$0 * 2]) << 8 | UInt16(raw[$0 * 2 + 1]) }
        }
        let prefix = compressed(Array(groups.prefix(6)))
        let zone = parts.count > 1 ? "%\(parts[1])" : ""
        return (prefix.hasSuffix("::") ? prefix : prefix + ":") + "*:*" + zone
    }

    /// RFC 5952 text for the leading groups: lowercase hex, and the longest run
    /// of two or more zero groups collapsed to "::".
    private static func compressed(_ groups: [UInt16]) -> String {
        var best: Range<Int>?
        var start: Int?
        for index in 0...groups.count {
            if index < groups.count, groups[index] == 0 {
                if start == nil { start = index }
            } else if let runStart = start {
                if index - runStart >= 2, index - runStart > (best?.count ?? 0) { best = runStart..<index }
                start = nil
            }
        }
        let hex = groups.map { String($0, radix: 16) }
        guard let best else { return hex.joined(separator: ":") }
        let head = hex[..<best.lowerBound].joined(separator: ":")
        let tail = hex[best.upperBound...].joined(separator: ":")
        return head + "::" + tail
    }
}
