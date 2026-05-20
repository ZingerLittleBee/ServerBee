import Foundation
import UIKit

/// Generates and persists a user-editable device name for the mobile pair/login
/// flow. On iOS 16+, `UIDevice.current.name` returns the model literal (e.g.,
/// "iPhone") for apps without the `com.apple.developer.device-information.
/// user-assigned-device-name` entitlement, which makes every device's
/// `device_name` identical on the server side. This provider gives each
/// installation a stable, distinguishable name composed of model + iOS version
/// + a random 4-character suffix that is generated exactly once.
enum DeviceNameProvider {
    private static let storageKey = "deviceName"
    private static let suffixKey = "deviceNameSuffix"

    /// Returns the user-customised name if set, otherwise the auto-generated
    /// default. Always non-empty.
    @MainActor
    static func current(defaults: UserDefaults = .standard) -> String {
        if let stored = defaults.string(forKey: storageKey), !stored.isEmpty {
            return stored
        }
        return defaultName(defaults: defaults)
    }

    /// Persists a user-chosen name. Empty strings are coerced to the default.
    static func set(_ name: String, defaults: UserDefaults = .standard) {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            defaults.removeObject(forKey: storageKey)
        } else {
            defaults.set(trimmed, forKey: storageKey)
        }
    }

    /// The auto-generated default. Stable across calls because the suffix is
    /// persisted on first read.
    @MainActor
    static func defaultName(defaults: UserDefaults = .standard) -> String {
        let suffix = stableSuffix(defaults: defaults)
        let model = UIDevice.current.model
        let version = UIDevice.current.systemVersion
        return "\(model) \(version) (\(suffix))"
    }

    private static func stableSuffix(defaults: UserDefaults) -> String {
        if let existing = defaults.string(forKey: suffixKey), existing.count == 4 {
            return existing
        }
        let alphabet = Array("ABCDEFGHJKLMNPQRSTUVWXYZ23456789")
        let generated = String((0 ..< 4).map { _ in alphabet.randomElement()! })
        defaults.set(generated, forKey: suffixKey)
        return generated
    }
}
