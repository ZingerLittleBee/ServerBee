import Foundation
import SwiftUI

@Observable
@MainActor
final class AuthManager {
    private let refreshCoordinator: RefreshCoordinator
    private let revocations: PendingSessionRevocations
    private let authenticationStorage: any MobileAuthenticationStorage
    private let cleanupSession: URLSession
    private var saved: SavedMobileAuthentication?
    private var storageLoaded = false
    private var retryTask: Task<Void, Never>?
    private var ending = Set<UUID>()
    private var recoveryStatusOwner = UUID()
    private var recoveryCandidates: [MobileRecoveryCandidate] = []
    private(set) var authenticationGeneration = UUID()
    private(set) var recoveryError: String?
    private(set) var sessionRecovery: SessionRecoveryIdentity?
    var isLoading = true
    var isAuthenticated = false
    var user: MobileUser?
    var serverUrl: String? {
        didSet { if serverUrl != oldValue { authenticationGeneration = UUID() } }
    }

    init(refreshCoordinator: RefreshCoordinator = RefreshCoordinator(),
         revocations: PendingSessionRevocations = PendingSessionRevocations(),
         authenticationStorage: any MobileAuthenticationStorage = PrivateMobileAuthenticationStorage(),
         cleanupSession: URLSession = APIClient.makeCleanupSession()) {
        self.refreshCoordinator = refreshCoordinator
        self.revocations = revocations
        self.authenticationStorage = authenticationStorage
        self.cleanupSession = cleanupSession
    }

    func initialize() async {
        isLoading = true
        defer { isLoading = false }
        #if DEBUG
        if let seed = UITestSupport.seed {
            setServerUrl(seed.serverUrl)
            if let installation = seed.installationId { try? KeychainService.saveString(installation, for: KeychainService.installationIdKey) }
            handleLoginResponse(MobileTokenResponse(accessToken: seed.accessToken, accessExpiresInSecs: 900,
                refreshToken: seed.refreshToken, refreshExpiresInSecs: 3600, tokenType: "Bearer",
                user: MobileUser(id: seed.userId, username: seed.username, role: seed.role)))
            if UITestSupport.sessionRecovery, let context = captureContext() {
                do { try suspendSessionForRecovery(context: context) } catch { recoveryError = error.localizedDescription }
            }
            return
        }
        #endif
        do {
            try loadAuthenticationIfNeeded()
            guard let restored = saved else {
                serverUrl = KeychainService.loadString(for: KeychainService.serverUrlKey)
                isLoading = false
                await retryPendingRevocations()
                return
            }
            serverUrl = restored.serverUrl
            authenticationGeneration = restored.loginId
            if try revocations.records().contains(where: { $0.id == restored.loginId }) {
                try clearPersistedAuthentication()
                isLoading = false
                await retryPendingRevocations()
                return
            }
            if restored.requiresSessionRecovery == true {
                suspendSession(restored)
                await retryPendingRevocations()
                return
            }
            user = restored.user
            isAuthenticated = true
            guard let context = captureContext() else { throw AuthError.staleIdentity }
            do {
                _ = try await refreshAccessToken(context: context)
            } catch AuthError.staleIdentity {
                return
            } catch AuthError.refreshUnauthorized {
                await endSession(context: context, authenticationRejected: true)
            } catch {
                await endSession(context: context)
            }
            await retryPendingRevocations()
        } catch { recoveryError = error.localizedDescription; isAuthenticated = false }
    }

    nonisolated static func readAuthentication() throws -> SavedMobileAuthentication? {
        guard let data = try KeychainService.readThrowing(for: SavedMobileAuthentication.key) else { return nil }
        return try JSONDecoder().decode(SavedMobileAuthentication.self, from: data)
    }

    func setServerUrl(_ url: String) {
        serverUrl = url
        try? KeychainService.saveString(url, for: KeychainService.serverUrlKey)
    }

    func prepareForLogin() async throws {
        try loadAuthenticationIfNeeded()
        let generation = authenticationGeneration
        await retryPendingRevocations()
        guard authenticationGeneration == generation else { throw AuthError.staleIdentity }
        if let saved {
            guard let record = saved.revocation else { throw AuthError.secureLogoutNeedsConnection }
            try revocations.enqueue(record)
            // The initial capacity-recovery pass predates this record. Attempt
            // this exact identity before a replacement can create push ownership.
            do {
                try await APIClient.revokeSavedSession(record, session: cleanupSession)
                try revocations.remove(record)
            } catch { /* Retain the exact proof for the next bounded retry. */ }
        }
        guard authenticationGeneration == generation else { throw AuthError.staleIdentity }
        try revocations.requireLoginCapacity()
    }

    /// Publish a fresh identity only after its complete normal-auth record commits.
    func handleLoginResponse(_ response: MobileTokenResponse, origin: String? = nil,
                             installationId: String? = nil, expectedGeneration: UUID? = nil) {
        do {
            if let expectedGeneration, authenticationGeneration != expectedGeneration { throw AuthError.staleIdentity }
            recoveryStatusOwner = UUID()
            if let session = response.mobileSessionId, UUID(uuidString: session) == nil { throw AuthError.invalidSessionResponse }
            try loadAuthenticationIfNeeded()
            guard let serverUrl = origin ?? serverUrl else { throw AuthError.noServerUrl }
            let installation = try installationId ?? InstallationID.getOrCreateThrowing()
            if let previous = saved {
                guard let record = previous.revocation else { throw AuthError.secureLogoutNeedsConnection }
                try revocations.enqueue(record)
            }
            try revocations.requireLoginCapacity()
            let value = SavedMobileAuthentication(loginId: UUID(), serverUrl: serverUrl,
                installationId: installation, user: response.user,
                accessToken: response.accessToken, refreshToken: response.refreshToken,
                revocationToken: response.revocationToken, mobileSessionId: response.mobileSessionId,
                confirmedDeletionProof: response.mobileSessionId == nil ? nil : response.revocationToken)
            try writeAuthentication(value)
            saved = value
            SharedPushKeychain.delete()
            self.serverUrl = value.serverUrl
            try? KeychainService.saveAtomically(Data(value.serverUrl.utf8), for: KeychainService.serverUrlKey)
            authenticationGeneration = value.loginId
            user = value.user
            isAuthenticated = true
            sessionRecovery = nil
            recoveryError = nil
        } catch AuthError.staleIdentity {
            // A later login owns the UI and credentials.
        } catch {
            recoveryError = error.localizedDescription
            isAuthenticated = false
            user = nil
        }
    }

    func getAccessToken() -> String? {
        guard sessionRecovery == nil else { return nil }
        return saved?.accessToken ?? KeychainService.loadString(for: KeychainService.accessTokenKey)
    }

    /// Low-level local reset. Production logout first persists a deletion record.
    func clearAuth() {
        do { try clearPersistedAuthentication() } catch { recoveryError = error.localizedDescription }
    }

    private func clearPersistedAuthentication() throws {
        for key in Self.legacyCredentialKeys { try KeychainService.deleteThrowing(for: key) }
        try authenticationStorage.delete()
        SharedPushKeychain.delete()
        saved = nil
        storageLoaded = true
        authenticationGeneration = UUID()
        user = nil
        isAuthenticated = false
        sessionRecovery = nil
        recoveryCandidates = []
    }

    func captureContext() -> MobileAuthenticationContext? {
        guard isAuthenticated, let serverUrl, let user, let token = getAccessToken(),
              saved == nil || (saved?.serverUrl == serverUrl && saved?.user.id == user.id) else { return nil }
        return MobileAuthenticationContext(serverUrl: serverUrl, userId: user.id,
            installationId: saved?.installationId ?? InstallationID.getOrCreate(), generation: authenticationGeneration,
            accessToken: token, revocationToken: saved?.revocationToken ?? saved?.refreshToken
                ?? KeychainService.loadString(for: KeychainService.revocationTokenKey)
                ?? KeychainService.loadString(for: KeychainService.refreshTokenKey),
            refreshToken: saved?.refreshToken ?? KeychainService.loadString(for: KeychainService.refreshTokenKey))
    }

    func isCurrent(_ context: MobileAuthenticationContext) -> Bool {
        isAuthenticated && serverUrl == context.serverUrl && user?.id == context.userId
            && authenticationGeneration == context.generation
            && (saved?.installationId ?? InstallationID.getOrCreate()) == context.installationId
    }
    func accessToken(ifCurrent context: MobileAuthenticationContext) -> String? { isCurrent(context) ? getAccessToken() : nil }

    func refreshAccessToken(context: MobileAuthenticationContext? = nil) async throws -> String {
        guard sessionRecovery == nil else { throw AuthError.secureLogoutNeedsConnection }
        if let context, !isCurrent(context) { throw AuthError.staleIdentity }
        let generation = authenticationGeneration
        let result = try await refreshCoordinator.refresh(generation: generation) { [self] in
            try await refreshCurrentIdentity(generation: generation)
        }
        guard authenticationGeneration == generation, result.generation == generation, sessionRecovery == nil else { throw AuthError.staleIdentity }
        return result.accessToken
    }

    /// Central gate for every mutation that can create a verified ownership row.
    func requireDeletionRecovery(context: MobileAuthenticationContext) async throws {
        guard isCurrent(context) else { throw AuthError.staleIdentity }
        if saved?.revocation == nil { _ = try await refreshAccessToken(context: context) }
        guard isCurrent(context), saved?.revocation != nil else { throw AuthError.secureLogoutNeedsConnection }
    }

    /// One bounded pass; persisted failures remain recoverable while signed out.
    func retryPendingRevocations() async {
        if let retryTask { await retryTask.value; return }
        let statusOwner = recoveryStatusOwner
        let task = Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                let records = try revocations.records().filter { !ending.contains($0.id) }
                await withTaskGroup(of: PendingSessionRevocation?.self) { group in
                    for record in records {
                        group.addTask { [cleanupSession] in
                            do { try await APIClient.revokeSavedSession(record, session: cleanupSession); return record } catch { return nil }
                        }
                    }
                    for await completed in group {
                        guard let completed else { continue }
                        do { try revocations.remove(completed) } catch { if recoveryStatusOwner == statusOwner { recoveryError = error.localizedDescription } }
                    }
                }
            } catch { if recoveryStatusOwner == statusOwner { recoveryError = error.localizedDescription } }
        }
        retryTask = task
        await task.value
        retryTask = nil
    }

    /// Explicit logout and every automatic-expiry path share this boundary.
    func endSession(context: MobileAuthenticationContext,
                    authenticationRejected: Bool = false,
                    beforeRevocation: (@MainActor () async -> Void)? = nil) async {
        guard !ending.contains(context.generation) else { return }
        ending.insert(context.generation)
        let statusOwner = UUID()
        if authenticationGeneration == context.generation { recoveryStatusOwner = statusOwner }
        defer { ending.remove(context.generation) }
        let original = saved.flatMap { $0.loginId == context.generation ? $0 : nil }
        do {
            if authenticationRejected { try suspendSessionForRecovery(context: context) }
            if let record = original?.revocation {
                try revocations.enqueue(record)
                await beforeRevocation?()
                if authenticationGeneration == context.generation { try clearPersistedAuthentication() }
                do {
                    try await APIClient.revokeSavedSession(record, session: cleanupSession)
                    try revocations.remove(record)
                } catch { /* Local logout succeeded; private proof remains for retry. */ }
            } else {
                // Unknown legacy material never enters the durable journal.
                await beforeRevocation?()
                do {
                    try await APIClient(authManager: self).revokeSession(context: context,
                        expectedSessionId: original?.mobileSessionId, session: cleanupSession)
                } catch {
                    if authenticationRejected || Self.isPermanentRejection(error) {
                        try suspendSessionForRecovery(context: context)
                    }
                    throw AuthError.secureLogoutNeedsConnection
                }
                if authenticationGeneration == context.generation { try clearPersistedAuthentication() }
            }
            if recoveryStatusOwner == statusOwner { recoveryError = nil }
        } catch { if recoveryStatusOwner == statusOwner { recoveryError = error.localizedDescription } }
    }

}

extension AuthManager {
    func accessTokenForReconnect() async -> String? {
        guard sessionRecovery == nil else { return nil }
        let context = captureContext()
        do { return try await refreshAccessToken() } catch AuthError.refreshUnauthorized {
            if let context { await endSession(context: context, authenticationRejected: true) }
            return nil
        } catch AuthError.staleIdentity { return nil } catch { return getAccessToken() }
    }

    /// Recovery is separate from normal login and never publishes new tokens.
    @discardableResult
    func recoverSession(username: String, password: String, totpCode: String? = nil,
                        selectedSessionId: String? = nil) async throws -> [MobileRecoveryCandidate] {
        guard var original = saved, sessionRecovery?.loginId == original.loginId,
              authenticationGeneration == original.loginId else { throw AuthError.staleIdentity }
        if let selectedSessionId, original.recoveryTargetSessionId == nil {
            guard recoveryCandidates.contains(where: { $0.mobileSessionId == selectedSessionId }) else {
                throw SessionRecoveryError.invalidConfirmation
            }
            // User-selected identity is durable before dispatch. A lost response
            // can then confirm exact absence even if a replacement login exists.
            original.selectedRecoverySessionId = selectedSessionId
            try writeAuthentication(original)
            saved = original
            sessionRecovery = SessionRecoveryIdentity(original)
        } else if let selectedSessionId, selectedSessionId != original.recoveryTargetSessionId {
            throw AuthError.staleIdentity
        }
        let result = try await SessionRecoveryClient.recover(original, username: username, password: password,
                                                             totpCode: totpCode, session: cleanupSession)
        guard authenticationGeneration == original.loginId, saved?.loginId == original.loginId,
              sessionRecovery?.loginId == original.loginId else { throw AuthError.staleIdentity }
        if result.outcome == .selectionRequired {
            recoveryCandidates = result.candidates ?? []
            return recoveryCandidates
        }
        try clearPersistedAuthentication()
        recoveryError = nil
        return []
    }

    /// Retry only the captured cleanup credentials, never ordinary auth traffic.
    func retrySessionCleanup() async throws {
        guard let original = saved, sessionRecovery?.loginId == original.loginId,
              authenticationGeneration == original.loginId else { throw AuthError.staleIdentity }
        let context = MobileAuthenticationContext(serverUrl: original.serverUrl, userId: original.user.id,
            installationId: original.installationId, generation: original.loginId, accessToken: original.accessToken,
            revocationToken: original.revocationToken, refreshToken: original.refreshToken)
        try await APIClient(authManager: self).revokeSession(context: context,
            expectedSessionId: original.recoveryTargetSessionId, session: cleanupSession)
        guard authenticationGeneration == original.loginId, saved?.loginId == original.loginId else { throw AuthError.staleIdentity }
        try clearPersistedAuthentication()
        recoveryError = nil
    }
}

private extension AuthManager {
    static func isPermanentRejection(_ error: Error) -> Bool {
        guard case APIError.httpError(let status, _) = error else { return false }
        return status == 401 || status == 403
    }

    func suspendSession(_ original: SavedMobileAuthentication) {
        sessionRecovery = SessionRecoveryIdentity(original)
        SharedPushKeychain.delete()
        isAuthenticated = false
        user = nil
    }

    func suspendSessionForRecovery(context: MobileAuthenticationContext) throws {
        guard authenticationGeneration == context.generation, var original = saved,
              original.loginId == context.generation else { return }
        // Suspend in memory even if Keychain is temporarily unwritable. Retain
        // the authoritative record until persistence or confirmed cleanup works.
        suspendSession(original)
        original.requiresSessionRecovery = true
        try writeAuthentication(original)
        saved = original
    }

    func refreshCurrentIdentity(generation: UUID) async throws -> String {
        guard authenticationGeneration == generation, sessionRecovery == nil else { throw AuthError.staleIdentity }
        try loadAuthenticationIfNeeded()
        guard var original = saved else { throw AuthError.refreshUnauthorized }
        if original.confirmedDeletionProof == nil, original.proposedDeletionProof == nil {
            // Existing normal-auth compatibility only; never journal this value.
            if original.revocationToken == nil { original.revocationToken = original.refreshToken }
            original.proposedDeletionProof = try SavedMobileAuthentication.proposedProof()
            try writeAuthentication(original)
            saved = original
        }
        guard let url = URL(string: "\(original.serverUrl)/api/mobile/auth/refresh") else { throw AuthError.noServerUrl }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder.snakeCase.encode(MobileRefreshRequest(refreshToken: original.refreshToken,
            installationId: original.installationId, revocationProof: original.proposedDeletionProof))
        let data: Data
        let response: URLResponse
        do { (data, response) = try await ServerHTTPTransport.data(for: request) } catch {
            guard authenticationGeneration == generation else { throw AuthError.staleIdentity }
            throw AuthError.refreshNetworkFailure(error)
        }
        guard authenticationGeneration == generation, serverUrl == original.serverUrl, sessionRecovery == nil else { throw AuthError.staleIdentity }
        guard let http = response as? HTTPURLResponse else { throw AuthError.refreshNetworkFailure(nil) }
        if http.statusCode == 401 || http.statusCode == 403 { throw AuthError.refreshUnauthorized }
        guard http.statusCode == 200 else { throw AuthError.refreshNetworkFailure(nil) }
        let tokens: MobileTokenResponse
        do { tokens = try JSONDecoder.snakeCase.decode(ApiResponse<MobileTokenResponse>.self, from: data).data } catch { throw AuthError.refreshNetworkFailure(error) }
        guard tokens.user.id == original.user.id else { throw AuthError.staleIdentity }
        if let session = tokens.mobileSessionId, UUID(uuidString: session) == nil { throw AuthError.invalidSessionResponse }
        if let known = original.mobileSessionId, let returned = tokens.mobileSessionId, known != returned {
            throw AuthError.staleIdentity
        }
        var rotated = original
        rotated.accessToken = tokens.accessToken
        rotated.refreshToken = tokens.refreshToken
        rotated.user = tokens.user
        if let proof = tokens.revocationToken, let session = tokens.mobileSessionId {
            guard original.proposedDeletionProof == nil || proof == original.proposedDeletionProof else { throw AuthError.staleIdentity }
            rotated.confirmedDeletionProof = proof
            rotated.mobileSessionId = session
            // Keep the original scope/legacy-cleanup input unchanged. The new
            // deletion capability has separate provenance and never rekeys push.
            rotated.proposedDeletionProof = nil
        }
        try writeAuthentication(rotated)
        saved = rotated
        user = rotated.user
        return rotated.accessToken
    }

    func loadAuthenticationIfNeeded() throws {
        guard !storageLoaded else { return }
        let value = try authenticationStorage.read() ?? migrateLegacyAuthentication()
        saved = value
        storageLoaded = true
    }

    func migrateLegacyAuthentication() throws -> SavedMobileAuthentication? {
        func string(_ key: String) throws -> String? {
            guard let bytes = try KeychainService.readThrowing(for: key) else { return nil }
            guard let value = String(data: bytes, encoding: .utf8) else { throw KeychainError.encodingFailed }
            return value
        }
        let access = try string(KeychainService.accessTokenKey)
        let refresh = try string(KeychainService.refreshTokenKey)
        let rawUser = try KeychainService.readThrowing(for: KeychainService.userKey)
        let proof = try string(KeychainService.revocationTokenKey)
        guard access != nil || refresh != nil || rawUser != nil || proof != nil else { return nil }
        guard let url = try string(KeychainService.serverUrlKey), let access, let refresh, let rawUser else {
            throw AuthError.secureLogoutNeedsConnection
        }
        let user = try JSONDecoder().decode(MobileUser.self, from: rawUser)
        guard let installation = try InstallationID.existingThrowing() else { throw AuthError.secureLogoutNeedsConnection }
        let value = SavedMobileAuthentication(loginId: UUID(), serverUrl: url,
            installationId: installation, user: user, accessToken: access,
            refreshToken: refresh, revocationToken: proof)
        try writeAuthentication(value)
        return value
    }

    func writeAuthentication(_ value: SavedMobileAuthentication) throws {
        try authenticationStorage.write(value)
        // The atomic snapshot is authoritative. Remove obsolete normal-auth copies.
        for key in Self.legacyCredentialKeys { try KeychainService.deleteThrowing(for: key) }
    }

    static let legacyCredentialKeys = [KeychainService.accessTokenKey, KeychainService.refreshTokenKey,
                                               KeychainService.revocationTokenKey, KeychainService.userKey]

}
