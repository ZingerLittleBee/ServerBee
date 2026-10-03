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
    var taskRun: TaskRunPushSummary?
    enum CodingKeys: String, CodingKey {
        case taskRun = "task_run"
        case kind
        case deploymentId = "deployment_id"
        case userId = "user_id"
        case installationId = "installation_id"
        case eventId = "event_id"
        case createdAt = "created_at"
        case expiresAt = "expires_at"
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

struct TaskRunPushSummary: Codable, Sendable, Equatable {
    let taskId: String
    let runId: String
    let total: Int
    let failed: Int
    let timedOut: Int
    let offline: Int
    let denied: Int
    enum CodingKeys: String, CodingKey {
        case total, failed, offline, denied
        case taskId = "task_id"
        case runId = "run_id"
        case timedOut = "timed_out"
    }
    var isValid: Bool {
        guard UUID(uuidString: taskId) != nil, UUID(uuidString: runId) != nil,
              total > 0, total <= 100_000,
              [failed, timedOut, offline, denied].allSatisfy({ $0 >= 0 && $0 <= total }) else { return false }
        let failures = failed + timedOut + offline + denied
        return failures > 0 && failures <= total
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
        try decode(envelope, key: key, now: now, requireUnexpired: true)
    }

    /// A displayed notification may be tapped later. Delivery expiry does not
    /// revoke its authenticated target; the Server still authorizes each read.
    static func decryptForNavigation(_ envelope: PushEnvelope, key: PushContentKey,
                                     now: Int64 = Int64(Date().timeIntervalSince1970)) throws -> PushContent {
        try decode(envelope, key: key, now: now, requireUnexpired: false)
    }

    private static func decode(_ envelope: PushEnvelope, key: PushContentKey, now: Int64, requireUnexpired: Bool) throws -> PushContent {
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
        guard content.deploymentId == key.deploymentId, content.userId == key.userId,
              content.installationId == key.installationId, try content.identity == envelope.identity,
              (!requireUnexpired || content.expiresAt > now), content.createdAt <= now + 60,
              content.expiresAt - content.createdAt == 1800 else { throw PushEnvelopeError.invalid }
        switch content.kind {
        case "test":
            guard UUID(uuidString: content.eventId) != nil, content.taskRun == nil else { throw PushEnvelopeError.invalid }
        case "task_failure":
            guard let run = content.taskRun, run.isValid,
                  content.eventId == run.runId else { throw PushEnvelopeError.invalid }
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
        if let run = content.taskRun {
            result.title = String(localized: "Task run failed")
            result.body = String(format: String(localized: "Targets: %lld. Failed: %lld. Timed out: %lld. Offline: %lld. Denied: %lld."),
                                 Int64(run.total), Int64(run.failed), Int64(run.timedOut), Int64(run.offline), Int64(run.denied))
        } else {
            result.title = String(localized: "Test notification")
            result.body = String(localized: "Your encrypted ServerBee notification is ready.")
        }
        result.userInfo = ["serverbee_target": target, "serverbee_key_id": key.keyId, "serverbee_envelope": object]
        return result
    }
}
