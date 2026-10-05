import Foundation

/// Provides a stable, unique installation identifier persisted in the Keychain.
///
/// On first call the ID is generated (UUID v4) and stored. Subsequent calls
/// return the same value, surviving app reinstalls as long as the Keychain
/// entry is not wiped.
enum InstallationID {
    static func existingThrowing() throws -> String? {
        guard let data = try KeychainService.readThrowing(for: KeychainService.installationIdKey) else { return nil }
        guard let value = String(data: data, encoding: .utf8), !value.isEmpty else { throw KeychainError.encodingFailed }
        return value
    }

    static func getOrCreateThrowing() throws -> String {
        if let existing = try existingThrowing() { return existing }
        let value = UUID().uuidString
        try KeychainService.saveAtomically(Data(value.utf8), for: KeychainService.installationIdKey)
        return value
    }

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
