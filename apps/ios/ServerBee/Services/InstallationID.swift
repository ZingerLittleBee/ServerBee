import Foundation

enum InstallationID: Sendable {
    private static let key = "serverbee_installation_id"

    static func getOrCreate() -> String {
        if let existing = KeychainService.load(key: key) {
            return existing
        }
        let newId = UUID().uuidString.lowercased()
        KeychainService.save(key: key, value: newId)
        return newId
    }
}
