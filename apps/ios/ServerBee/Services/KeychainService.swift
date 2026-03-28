import Foundation
import Security

/// A generic Keychain wrapper using Security.framework.
///
/// All items are stored as `kSecClassGenericPassword` entries under the
/// `com.serverbee.mobile` service namespace.
enum KeychainService {
    // MARK: - Keys

    static let accessTokenKey = "serverbee_access_token"
    static let refreshTokenKey = "serverbee_refresh_token"
    static let userKey = "serverbee_user"
    static let serverUrlKey = "serverbee_server_url"
    static let installationIdKey = "serverbee_installation_id"

    private static let serviceName = "com.serverbee.mobile"

    // MARK: - Core Operations

    /// Save raw data to the Keychain for the given key.
    /// Updates the existing item if one already exists.
    static func save(_ data: Data, for key: String) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: key,
        ]

        // Delete any existing item first (SecItemUpdate sometimes fails on mismatched attrs).
        SecItemDelete(query as CFDictionary)

        var addQuery = query
        addQuery[kSecValueData as String] = data
        addQuery[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock

        let status = SecItemAdd(addQuery as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw KeychainError.saveFailed(status)
        }
    }

    /// Load raw data from the Keychain for the given key.
    /// Returns `nil` if the item does not exist.
    static func load(for key: String) -> Data? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: key,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]

        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)

        guard status == errSecSuccess else {
            return nil
        }

        return result as? Data
    }

    /// Delete an item from the Keychain for the given key.
    static func delete(for key: String) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: key,
        ]

        SecItemDelete(query as CFDictionary)
    }

    // MARK: - String Convenience

    /// Save a UTF-8 string to the Keychain.
    static func saveString(_ value: String, for key: String) throws {
        guard let data = value.data(using: .utf8) else {
            throw KeychainError.encodingFailed
        }
        try save(data, for: key)
    }

    /// Load a UTF-8 string from the Keychain.
    static func loadString(for key: String) -> String? {
        guard let data = load(for: key) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    // MARK: - Codable Convenience

    /// Encode a `Codable` value to JSON and save it to the Keychain.
    static func saveCodable<T: Encodable>(_ value: T, for key: String) throws {
        let data = try JSONEncoder().encode(value)
        try save(data, for: key)
    }

    /// Load and decode a `Codable` value from the Keychain.
    static func loadCodable<T: Decodable>(for key: String) -> T? {
        guard let data = load(for: key) else { return nil }
        return try? JSONDecoder().decode(T.self, from: data)
    }
}

// MARK: - Errors

enum KeychainError: Error, LocalizedError {
    case saveFailed(OSStatus)
    case encodingFailed

    var errorDescription: String? {
        switch self {
        case .saveFailed(let status):
            return "Keychain save failed with status: \(status)"
        case .encodingFailed:
            return "Failed to encode value for Keychain storage"
        }
    }
}
