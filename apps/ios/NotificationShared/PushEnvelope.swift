import CryptoKit
import Foundation
import Security
import UserNotifications

struct PushEnvelope: Codable, Sendable {
    let version: Int
    let keyId: String
    let identity: String
    let nonce: String
    let ciphertext: String
    enum CodingKeys: String, CodingKey {
        case version, identity, nonce, ciphertext
        case keyId = "key_id"
    }
}

struct PushContent: Codable, Sendable, Equatable {
    let kind: String
    let deploymentId: String
    let userId: String
    let installationId: String
    let eventId: String
    let createdAt: Int64
    let expiresAt: Int64
    var serverId: String?
    var securityEventId: String?
    var securityEventType: String?
    enum CodingKeys: String, CodingKey {
        case kind
        case deploymentId = "deployment_id"
        case userId = "user_id"
        case installationId = "installation_id"
        case eventId = "event_id"
        case createdAt = "created_at"
        case expiresAt = "expires_at"
        case serverId = "server_id"
        case securityEventId = "security_event_id"
        case securityEventType = "security_event_type"
    }
    var identity: String {
        get throws {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.withoutEscapingSlashes]
            let data = try encoder.encode([deploymentId, userId, installationId])
            return SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        }
    }
}

struct PushContentKey: Codable, Sendable {
    static let storageKey = "serverbee_active_push_content_key"
    let keyId: String
    let key: String
    let deploymentId: String
    let userId: String
    let installationId: String
    let scope: String
    enum CodingKeys: String, CodingKey {
        case key, scope
        case keyId = "key_id"
        case deploymentId = "deployment_id"
        case userId = "user_id"
        case installationId = "installation_id"
    }
}

enum PushEnvelopeError: Error { case invalid }

enum PushEnvelopeDecoder {
    static func decrypt(_ envelope: PushEnvelope, key: PushContentKey, now: Int64 = Int64(Date().timeIntervalSince1970)) throws -> PushContent {
        let content = try authenticatedContent(envelope, key: key, now: now)
        guard content.expiresAt > now else { throw PushEnvelopeError.invalid }
        return content
    }

    /// Delivery expires after 30 minutes. An already-presented security target
    /// remains navigable, subject to the app's current login and Server checks.
    static func decryptForNavigation(_ envelope: PushEnvelope, key: PushContentKey, now: Int64 = Int64(Date().timeIntervalSince1970)) throws -> PushContent {
        let content = try authenticatedContent(envelope, key: key, now: now)
        guard content.kind == "security" || content.expiresAt > now else { throw PushEnvelopeError.invalid }
        return content
    }

    private static func authenticatedContent(_ envelope: PushEnvelope, key: PushContentKey, now: Int64) throws -> PushContent {
        guard envelope.version == 1, envelope.keyId == key.keyId, envelope.ciphertext.count <= 2760,
              let secret = Data(base64Encoded: key.key), secret.count == 32,
              let nonce = Data(base64Encoded: envelope.nonce), nonce.count == 12,
              let ciphertext = Data(base64Encoded: envelope.ciphertext), ciphertext.count >= 16, ciphertext.count <= 2064 else {
            throw PushEnvelopeError.invalid
        }
        let aad = Data("ServerBee.Push.v1|\(envelope.keyId)|\(envelope.identity)".utf8)
        let sealed = try AES.GCM.SealedBox(nonce: AES.GCM.Nonce(data: nonce), ciphertext: ciphertext.dropLast(16), tag: ciphertext.suffix(16))
        let bytes = try AES.GCM.open(sealed, using: SymmetricKey(data: secret), authenticating: aad)
        let content = try JSONDecoder().decode(PushContent.self, from: bytes)
        let lifetime = content.expiresAt.subtractingReportingOverflow(content.createdAt)
        let latestCreation = now.addingReportingOverflow(60)
        guard content.deploymentId == key.deploymentId, content.userId == key.userId,
              content.installationId == key.installationId, try content.identity == envelope.identity,
              UUID(uuidString: content.eventId) != nil,
              !latestCreation.overflow, content.createdAt <= latestCreation.partialValue,
              !lifetime.overflow, lifetime.partialValue == 1800 else { throw PushEnvelopeError.invalid }
        switch content.kind {
        case "test":
            guard content.serverId == nil, content.securityEventId == nil, content.securityEventType == nil else { throw PushEnvelopeError.invalid }
        case "security":
            guard let serverId = content.serverId, UUID(uuidString: serverId) != nil,
                  content.securityEventId == content.eventId,
                  let eventType = content.securityEventType, ["ssh_login", "ssh_brute_force", "port_scan"].contains(eventType) else { throw PushEnvelopeError.invalid }
        default: throw PushEnvelopeError.invalid
        }
        return content
    }
}

/// Shared only with the app and its Notification Service Extension. Keys are
/// device-only, not synchronized, and available after the first device unlock.
enum SharedPushKeychain {
    private static func query() -> [String: Any]? {
        guard let group = Bundle.main.object(forInfoDictionaryKey: "PushKeychainAccessGroup") as? String,
              !group.isEmpty, !group.contains("$(") else { return nil }
        return [kSecClass as String: kSecClassGenericPassword,
                kSecAttrService as String: "com.serverbee.mobile.push",
                kSecAttrAccount as String: PushContentKey.storageKey,
                kSecAttrAccessGroup as String: group]
    }
    static func load() -> Data? {
        guard var value = query() else { return nil }
        value[kSecReturnData as String] = true
        value[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: AnyObject?
        guard SecItemCopyMatching(value as CFDictionary, &result) == errSecSuccess else { return nil }
        return result as? Data
    }
    static func save(_ data: Data) throws {
        guard let value = query() else { throw PushEnvelopeError.invalid }
        let updates: [String: Any] = [kSecValueData as String: data,
                                     kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly]
        let status = SecItemUpdate(value as CFDictionary, updates as CFDictionary)
        if status == errSecItemNotFound {
            var added = value
            for (key, item) in updates { added[key] = item }
            guard SecItemAdd(added as CFDictionary, nil) == errSecSuccess else { throw PushEnvelopeError.invalid }
        } else if status != errSecSuccess { throw PushEnvelopeError.invalid }
    }
    static func delete() {
        guard let value = query() else { return }
        SecItemDelete(value as CFDictionary)
    }
}

/// The extension and boundary tests use the same render entry point. Only a
/// validated ciphertext can add a target. A fallback never trusts raw targets.
enum PushNotificationRenderer {
    static func render(_ input: UNNotificationContent, key: PushContentKey?, now: Int64 = Int64(Date().timeIntervalSince1970)) -> UNMutableNotificationContent {
        let result = UNMutableNotificationContent()
        result.title = String(localized: "ServerBee")
        result.body = String(localized: "Open ServerBee to view this notification.")
        result.sound = .default
        guard let key, let object = input.userInfo["serverbee_envelope"], JSONSerialization.isValidJSONObject(object),
              let bytes = try? JSONSerialization.data(withJSONObject: object), bytes.count <= 4096,
              let envelope = try? JSONDecoder().decode(PushEnvelope.self, from: bytes),
              let content = try? PushEnvelopeDecoder.decrypt(envelope, key: key, now: now),
              let targetData = try? JSONEncoder().encode(content),
              let target = try? JSONSerialization.jsonObject(with: targetData) else { return result }
        if content.kind == "security" {
            result.title = String(localized: "Security rule matched")
            switch content.securityEventType {
            case "ssh_login": result.body = String(localized: "An SSH login from a new IP matched a security rule.")
            case "ssh_brute_force": result.body = String(localized: "SSH brute-force activity matched a security rule.")
            default: result.body = String(localized: "Port-scan activity matched a security rule.")
            }
        } else {
            result.title = String(localized: "Test notification")
            result.body = String(localized: "Your encrypted ServerBee notification is ready.")
        }
        result.userInfo = ["serverbee_target": target, "serverbee_key_id": key.keyId, "serverbee_envelope": object]
        return result
    }
}
