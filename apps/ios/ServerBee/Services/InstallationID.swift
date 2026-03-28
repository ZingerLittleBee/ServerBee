import Foundation

/// Provides a stable, unique installation identifier persisted in the Keychain.
///
/// On first call the ID is generated (UUID v4) and stored. Subsequent calls
/// return the same value, surviving app reinstalls as long as the Keychain
/// entry is not wiped.
enum InstallationID {
    /// Returns the existing installation ID or creates and persists a new one.
    static func getOrCreate() -> String {
        if let existing = KeychainService.loadString(for: KeychainService.installationIdKey) {
            return existing
        }
        let newId = UUID().uuidString
        try? KeychainService.saveString(newId, for: KeychainService.installationIdKey)
        return newId
    }
}
