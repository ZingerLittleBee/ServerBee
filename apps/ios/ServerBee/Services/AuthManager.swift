import Foundation
import SwiftUI

@Observable
@MainActor
final class AuthManager {
    private let refreshCoordinator: RefreshCoordinator
    private let revocations: PendingSessionRevocations
    private let authenticationStorage: any MobileAuthenticationStorage
    private var saved: SavedMobileAuthentication?
    private var storageLoaded = false
    private var retryTask: Task<Void, Never>?
    private var ending = Set<UUID>()
    private var recoveryStatusOwner = UUID()
    private(set) var authenticationGeneration = UUID()
    private(set) var recoveryError: String?
    var isLoading = true
    var isAuthenticated = false
    var user: MobileUser?
    var serverUrl: String? {
        didSet { if serverUrl != oldValue { authenticationGeneration = UUID() } }
    }

    init(refreshCoordinator: RefreshCoordinator = RefreshCoordinator(),
         revocations: PendingSessionRevocations = PendingSessionRevocations(),
         authenticationStorage: any MobileAuthenticationStorage = PrivateMobileAuthenticationStorage()) {
        self.refreshCoordinator = refreshCoordinator
        self.revocations = revocations
        self.authenticationStorage = authenticationStorage
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
            user = restored.user
            isAuthenticated = true
            guard let context = captureContext() else { throw AuthError.staleIdentity }
            do { _ = try await refreshAccessToken(context: context) }
            catch AuthError.staleIdentity { return }
            catch { await endSession(context: context) }
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
                try await APIClient.revokeSavedSession(record)
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
            recoveryError = nil
        } catch AuthError.staleIdentity {
            // A later login owns the UI and credentials.
        } catch {
            recoveryError = error.localizedDescription
            isAuthenticated = false
            user = nil
        }
    }

    func getAccessToken() -> String? { saved?.accessToken ?? KeychainService.loadString(for: KeychainService.accessTokenKey) }

    /// Low-level local reset. Production logout first persists a deletion record.
    func clearAuth() {
        do { try clearPersistedAuthentication() }
        catch { recoveryError = error.localizedDescription }
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
        if let context, !isCurrent(context) { throw AuthError.staleIdentity }
        let generation = authenticationGeneration
        let result = try await refreshCoordinator.refresh(generation: generation) { [self] in
            try await refreshCurrentIdentity(generation: generation)
        }
        guard authenticationGeneration == generation, result.generation == generation else { throw AuthError.staleIdentity }
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
                        group.addTask {
                            do { try await APIClient.revokeSavedSession(record); return record }
                            catch { return nil }
                        }
                    }
                    for await completed in group {
                        guard let completed else { continue }
                        do { try revocations.remove(completed) }
                        catch { if recoveryStatusOwner == statusOwner { recoveryError = error.localizedDescription } }
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
                    beforeRevocation: (@MainActor () async -> Void)? = nil) async {
        guard !ending.contains(context.generation) else { return }
        ending.insert(context.generation)
        let statusOwner = UUID()
        if authenticationGeneration == context.generation { recoveryStatusOwner = statusOwner }
        defer { ending.remove(context.generation) }
        let original = saved.flatMap { $0.loginId == context.generation ? $0 : nil }
        do {
            if let record = original?.revocation {
                try revocations.enqueue(record)
                await beforeRevocation?()
                if authenticationGeneration == context.generation { try clearPersistedAuthentication() }
                do {
                    try await APIClient.revokeSavedSession(record)
                    try revocations.remove(record)
                } catch { /* Local logout succeeded; private proof remains for retry. */ }
            } else {
                // Unknown legacy material never enters the durable journal.
                await beforeRevocation?()
                do { try await APIClient(authManager: self).revokeSession(context: context) }
                catch { throw AuthError.secureLogoutNeedsConnection }
                if authenticationGeneration == context.generation { try clearPersistedAuthentication() }
            }
            if recoveryStatusOwner == statusOwner { recoveryError = nil }
        } catch { if recoveryStatusOwner == statusOwner { recoveryError = error.localizedDescription } }
    }

    func accessTokenForReconnect() async -> String? {
        let context = captureContext()
        do { return try await refreshAccessToken() }
        catch AuthError.refreshUnauthorized { if let context { await endSession(context: context) }; return nil }
        catch AuthError.staleIdentity { return nil }
        catch { return getAccessToken() }
    }
}

private extension AuthManager {
    func refreshCurrentIdentity(generation: UUID) async throws -> String {
        guard authenticationGeneration == generation else { throw AuthError.staleIdentity }
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
        do { (data, response) = try await ServerHTTPTransport.data(for: request) }
        catch {
            guard authenticationGeneration == generation else { throw AuthError.staleIdentity }
            throw AuthError.refreshNetworkFailure(error)
        }
        guard authenticationGeneration == generation, serverUrl == original.serverUrl else { throw AuthError.staleIdentity }
        guard let http = response as? HTTPURLResponse else { throw AuthError.refreshNetworkFailure(nil) }
        if http.statusCode == 401 || http.statusCode == 403 { throw AuthError.refreshUnauthorized }
        guard http.statusCode == 200 else { throw AuthError.refreshNetworkFailure(nil) }
        let tokens: MobileTokenResponse
        do { tokens = try JSONDecoder.snakeCase.decode(ApiResponse<MobileTokenResponse>.self, from: data).data }
        catch { throw AuthError.refreshNetworkFailure(error) }
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

// MARK: - Refresh Coordinator

struct ScopedAccessToken: Sendable {
    let generation: UUID
    let accessToken: String
}

/// Serialises concurrent token-refresh attempts.
///
/// Semantics:
/// - For each login generation at most one `refreshFn` is in flight.
/// - While a refresh is in flight, callers for the same login `await` on the existing
///   task so we don't hammer the refresh endpoint or burn a one-time-use
///   refresh token.
/// - **On success:** every waiter receives the new access token.
/// - **On failure:** the in-flight attempt's error is propagated ONLY to the
///   caller who initiated it. Subsequent waiters are released and each gets a
///   fresh attempt at `refreshFn`. This lets a transient network failure for
///   the first caller not penalise queued callers — the next one retries.
///
/// Internal so tests can drive `refresh(generation:using:)` directly without going
/// through `AuthManager.refreshAccessToken()` — see RefreshCoordinatorTests.
actor RefreshCoordinator {
    private var inFlight: [UUID: (id: UUID, task: Task<ScopedAccessToken, Error>)] = [:]
    // A scheduler boundary permits deterministic tests of a completed task
    // whose owner has not resumed to remove it. Production does not suspend here.
    private let beforeCompletion: (@Sendable (ScopedAccessToken) async -> Void)?

    init(beforeCompletion: (@Sendable (ScopedAccessToken) async -> Void)? = nil) {
        self.beforeCompletion = beforeCompletion
    }

    func refresh(
        generation: UUID,
        using refreshFn: @Sendable @escaping () async throws -> String
    ) async throws -> ScopedAccessToken {
        while let existing = inFlight[generation] {
            do {
                return try await existing.task.value
            } catch {
                if inFlight[generation]?.id == existing.id { inFlight[generation] = nil }
            }
        }

        let id = UUID()
        let task = Task {
            let token = try await refreshFn()
            return ScopedAccessToken(generation: generation, accessToken: token)
        }
        inFlight[generation] = (id, task)
        do {
            let result = try await task.value
            if let beforeCompletion { await beforeCompletion(result) }
            if inFlight[generation]?.id == id { inFlight[generation] = nil }
            return result
        } catch {
            if inFlight[generation]?.id == id { inFlight[generation] = nil }
            throw error
        }
    }
}

// MARK: - Auth Errors

enum AuthError: Error, LocalizedError {
    case noServerUrl
    case staleIdentity
    case refreshUnauthorized           // server returned 401 — credentials revoked
    case refreshNetworkFailure(Error?) // transient: no network, 5xx, timeout
    case invalidCredentials
    case twoFactorRequired
    case tooManyAttempts
    case networkError(Error)
    case secureLogoutNeedsConnection
    case cleanupCapacity
    case invalidSessionResponse

    var errorDescription: String? {
        switch self {
        case .invalidSessionResponse:
            return String(localized: "The server returned an invalid session identity. Please retry.")
        case .secureLogoutNeedsConnection:
            return String(localized: "Connect to your server to complete secure sign-out before signing in again.")
        case .cleanupCapacity:
            return String(localized: "Pending sign-outs need to finish before another login. Connect to your servers and retry.")
        case .noServerUrl:
            return String(localized: "No server URL configured")
        case .refreshUnauthorized, .staleIdentity:
            return String(localized: "Session expired. Please log in again.")
        case .refreshNetworkFailure:
            return String(localized: "Could not reach the server. Please check your connection.")
        case .invalidCredentials:
            return String(localized: "Invalid username or password")
        case .twoFactorRequired:
            return String(localized: "Two-factor authentication is required")
        case .tooManyAttempts:
            return String(localized: "Too many attempts. Please try again later.")
        case .networkError(let error):
            return String(localized: "Network error: \(error.localizedDescription)")
        }
    }
}

/// Immutable identity for an in-flight mobile registration or logout request.
struct MobileAuthenticationContext: Sendable {
    let serverUrl: String
    let userId: String
    let installationId: String
    let generation: UUID
    let accessToken: String
    let revocationToken: String?
    // The saved proof may belong to a session replaced by an older Server.
    // Capture this login's current secret before rotation can lose its response.
    // It is sent only to the deletion endpoint, never as an access credential.
    let refreshToken: String?
}
