import Foundation
import Security

/// A capability that can delete one exact Server session, never authenticate it.
struct PendingSessionRevocation: Codable, Sendable, Equatable, Identifiable {
    let id: UUID
    let serverUrl: String
    let userId: String
    let installationId: String
    let mobileSessionId: String
    let proof: String
}

@MainActor
protocol SessionRevocationStorage {
    func read() throws -> [PendingSessionRevocation]
    func write(_ records: [PendingSessionRevocation]) throws
}

@MainActor
struct PrivateSessionRevocationStorage: SessionRevocationStorage {
    static let key = "serverbee_pending_session_revocations_v1"
    func read() throws -> [PendingSessionRevocation] {
        guard let bytes = try KeychainService.readThrowing(for: Self.key) else { return [] }
        return try JSONDecoder().decode([PendingSessionRevocation].self, from: bytes)
    }
    func write(_ records: [PendingSessionRevocation]) throws {
        try KeychainService.saveAtomically(JSONEncoder().encode(records), for: Self.key)
    }
}

/// Small logout-specific journal. Capacity never evicts an unresolved proof.
@MainActor
final class PendingSessionRevocations {
    static let capacity = 16
    private let storage: any SessionRevocationStorage
    init(storage: any SessionRevocationStorage = PrivateSessionRevocationStorage()) { self.storage = storage }
    func records() throws -> [PendingSessionRevocation] { try storage.read() }
    func requireLoginCapacity() throws {
        guard try records().count < Self.capacity else { throw AuthError.cleanupCapacity }
    }
    func enqueue(_ record: PendingSessionRevocation) throws {
        var current = try records()
        if let existing = current.first(where: { $0.id == record.id }) {
            guard existing == record else { throw AuthError.staleIdentity }
            return
        }
        guard current.count < Self.capacity else { throw AuthError.cleanupCapacity }
        current.append(record)
        try storage.write(current)
    }
    func remove(_ record: PendingSessionRevocation) throws {
        var current = try records()
        current.removeAll { $0 == record }
        try storage.write(current)
    }
}

/// Authoritative normal-auth storage is one atomic item. Legacy per-field keys
/// are migration inputs, never proof provenance. General credentials stay here,
/// and are never copied into PendingSessionRevocations.
struct SavedMobileAuthentication: Codable, Sendable {
    let loginId: UUID
    let serverUrl: String
    let installationId: String
    var user: MobileUser
    var accessToken: String
    var refreshToken: String
    var revocationToken: String?
    var mobileSessionId: String?
    var confirmedDeletionProof: String?
    var proposedDeletionProof: String?
    static let key = "serverbee_authentication_v1"
    var revocation: PendingSessionRevocation? {
        guard let mobileSessionId, UUID(uuidString: mobileSessionId) != nil,
              let proof = confirmedDeletionProof, !proof.isEmpty else { return nil }
        return PendingSessionRevocation(id: loginId, serverUrl: serverUrl, userId: user.id,
                                        installationId: installationId, mobileSessionId: mobileSessionId, proof: proof)
    }

    static func proposedProof() throws -> String {
        var bytes = [UInt8](repeating: 0, count: 32)
        let status = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        guard status == errSecSuccess else { throw KeychainError.saveFailed(status) }
        return "sb-revoke-v1." + Data(bytes).base64EncodedString()
            .replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}

@MainActor
protocol MobileAuthenticationStorage {
    func read() throws -> SavedMobileAuthentication?
    func write(_ value: SavedMobileAuthentication) throws
    func delete() throws
}

@MainActor
struct PrivateMobileAuthenticationStorage: MobileAuthenticationStorage {
    func read() throws -> SavedMobileAuthentication? { try AuthManager.readAuthentication() }
    func write(_ value: SavedMobileAuthentication) throws {
        try KeychainService.saveAtomically(JSONEncoder().encode(value), for: SavedMobileAuthentication.key)
    }
    func delete() throws { try KeychainService.deleteThrowing(for: SavedMobileAuthentication.key) }
}
