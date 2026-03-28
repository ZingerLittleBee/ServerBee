import Foundation

enum InstallationID: Sendable {
    private static let keychainKey = "installation_id"

    static func getOrCreate() -> String {
        if let existing = KeychainService.loadString(key: keychainKey) {
            return existing
        }
        let newID = UUID().uuidString.lowercased()
        KeychainService.saveString(key: keychainKey, value: newID)
        return newID
    }
}
