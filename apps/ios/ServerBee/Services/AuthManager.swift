import Foundation
import SwiftUI

/// Manages authentication state for the mobile app.
///
/// Isolated to `@MainActor` so that `@Observable` state is mutated only on the
/// main thread. Background callers (`APIClient` actor, `WebSocketClient`) hop
/// via `await` to read `serverUrl` / call `getAccessToken()`.
@Observable
@MainActor
final class AuthManager {
    // MARK: - Private

    private let refreshCoordinator: RefreshCoordinator

    init(refreshCoordinator: RefreshCoordinator = RefreshCoordinator()) {
        self.refreshCoordinator = refreshCoordinator
    }
    private(set) var authenticationGeneration = UUID()

    // MARK: - Published State

    var isLoading = true
    var isAuthenticated = false
    var user: MobileUser?
    var serverUrl: String? {
        didSet {
            if serverUrl != oldValue { authenticationGeneration = UUID() }
        }
    }

    // MARK: - Lifecycle

    /// Called once on app launch. Restores Keychain state and validates the session.
    func initialize() async {
        isLoading = true
        defer { isLoading = false }

        #if DEBUG
        if let seed = UITestSupport.seed {
            serverUrl = seed.serverUrl
            try? KeychainService.saveString(seed.serverUrl, for: KeychainService.serverUrlKey)
            try? KeychainService.saveString(seed.accessToken, for: KeychainService.accessTokenKey)
            try? KeychainService.saveString(seed.refreshToken, for: KeychainService.refreshTokenKey)
            if let installationId = seed.installationId {
                try? KeychainService.saveString(installationId, for: KeychainService.installationIdKey)
            }
            let seededUser = MobileUser(id: seed.userId, username: seed.username, role: seed.role)
            try? KeychainService.saveCodable(seededUser, for: KeychainService.userKey)
            user = seededUser
            isAuthenticated = true
            return
        }
        #endif

        // Restore server URL
        serverUrl = KeychainService.loadString(for: KeychainService.serverUrlKey)

        // Capture restored identity before refresh can rotate Server state.
        guard let accessToken = getAccessToken(), let serverUrl,
              let savedUser: MobileUser = KeychainService.loadCodable(for: KeychainService.userKey),
              let refreshToken = KeychainService.loadString(for: KeychainService.refreshTokenKey) else { return }
        let restored = MobileAuthenticationContext(
            serverUrl: serverUrl, userId: savedUser.id, installationId: InstallationID.getOrCreate(),
            generation: authenticationGeneration, accessToken: accessToken,
            revocationToken: KeychainService.loadString(for: KeychainService.revocationTokenKey) ?? refreshToken
        )
        do {
            let response = try await refreshTokens(refreshToken: refreshToken)
            guard response.user.id == savedUser.id else { throw AuthError.staleIdentity }
            persistTokens(response)
        } catch AuthError.staleIdentity {
            return
        } catch {
            // A committed rotation with a lost response can leave a registration
            // alive. Revoke that original login before discarding its local proof.
            await endSession(context: restored)
        }
    }

    // MARK: - Server URL

    /// Persist the server base URL (e.g. `https://my-server.example.com:9527`).
    func setServerUrl(_ url: String) {
        serverUrl = url
        try? KeychainService.saveString(url, for: KeychainService.serverUrlKey)
    }

    // MARK: - Login Handling

    /// Persist tokens & user from a successful fresh login response.
    func handleLoginResponse(_ response: MobileTokenResponse) {
        authenticationGeneration = UUID()
        // A fresh login owns a fresh stable revocation credential.
        KeychainService.delete(for: KeychainService.revocationTokenKey)
        persistTokens(response)
    }

    private func persistTokens(_ response: MobileTokenResponse) {
        if let credential = response.revocationToken {
            try? KeychainService.saveString(credential, for: KeychainService.revocationTokenKey)
        }
        try? KeychainService.saveString(response.accessToken, for: KeychainService.accessTokenKey)
        try? KeychainService.saveString(response.refreshToken, for: KeychainService.refreshTokenKey)
        try? KeychainService.saveCodable(response.user, for: KeychainService.userKey)
        user = response.user
        isAuthenticated = true
    }

    // MARK: - Token Access

    /// Read the current access token from the Keychain.
    func getAccessToken() -> String? {
        KeychainService.loadString(for: KeychainService.accessTokenKey)
    }

    // MARK: - Logout

    /// Clear the user's authenticated session.
    ///
    /// **Cleared:**
    /// - Access token (Keychain)
    /// - Refresh token (Keychain)
    /// - Persisted `MobileUser` (Keychain)
    /// - In-memory `user` and `isAuthenticated`
    ///
    /// **Preserved on purpose:**
    /// - `serverUrl` — the user will likely log back into the same server,
    ///    so we pre-fill the login form rather than forcing them to retype it.
    /// - `installationId` — a stable device identifier; rotating it would
    ///    desynchronise push-notification routing and would make the server
    ///    think this is a brand-new device on next login.
    ///
    /// If you need a hard reset (e.g. "Forget this server" affordance), add a
    /// separate `forgetServer()` API rather than expanding this method.
    func clearAuth() {
        authenticationGeneration = UUID()
        KeychainService.delete(for: KeychainService.accessTokenKey)
        KeychainService.delete(for: KeychainService.refreshTokenKey)
        KeychainService.delete(for: KeychainService.revocationTokenKey)
        KeychainService.delete(for: KeychainService.userKey)
        user = nil
        isAuthenticated = false
    }

    // MARK: - Token Refresh (public, coalesced)

    /// Centralized token refresh. Both APIClient (on 401) and WebSocketClient
    /// (on reconnect) call this. Concurrent calls are coalesced by RefreshCoordinator.
    func refreshAccessToken(context: MobileAuthenticationContext? = nil) async throws -> String {
        if let context, !isCurrent(context) { throw AuthError.staleIdentity }
        let generation = authenticationGeneration
        let result = try await refreshCoordinator.refresh(generation: generation) { [self] in
            try await refreshCurrentIdentity(generation: generation)
        }
        guard authenticationGeneration == generation, result.generation == generation else { throw AuthError.staleIdentity }
        return result.accessToken
    }

    private func refreshCurrentIdentity(generation: UUID) async throws -> String {
        guard authenticationGeneration == generation else { throw AuthError.staleIdentity }
        guard let refreshToken = KeychainService.loadString(for: KeychainService.refreshTokenKey) else {
            throw AuthError.refreshUnauthorized
        }
        let response = try await refreshTokens(refreshToken: refreshToken)
        guard authenticationGeneration == generation else { throw AuthError.staleIdentity }
        // Ordinary rotation belongs to the same login; it must not invalidate
        // captured registration context or adopt a replacement account.
        persistTokens(response)
        return response.accessToken
    }

    /// Capture deployment, account, installation and login generation together.
    func captureContext() -> MobileAuthenticationContext? {
        guard isAuthenticated, let serverUrl, let user, let token = getAccessToken() else { return nil }
        return MobileAuthenticationContext(
            serverUrl: serverUrl, userId: user.id,
            installationId: InstallationID.getOrCreate(),
            generation: authenticationGeneration, accessToken: token,
            revocationToken: KeychainService.loadString(for: KeychainService.revocationTokenKey)
                ?? KeychainService.loadString(for: KeychainService.refreshTokenKey)
        )
    }

    func isCurrent(_ context: MobileAuthenticationContext) -> Bool {
        isAuthenticated && serverUrl == context.serverUrl && user?.id == context.userId
            && authenticationGeneration == context.generation
            && InstallationID.getOrCreate() == context.installationId
    }

    func accessToken(ifCurrent context: MobileAuthenticationContext) -> String? {
        isCurrent(context) ? getAccessToken() : nil
    }

    /// Every production auth-expiry path retains the original proof until
    /// its revocation attempt finishes, then clears only that login generation.
    func endSession(context: MobileAuthenticationContext) async {
        try? await APIClient(authManager: self).revokeSession(context: context)
        if authenticationGeneration == context.generation { clearAuth() }
    }

    /// Token for a WebSocket reconnect. A transient refresh failure (offline,
    /// timeout, 5xx) falls back to the stored token, so the socket keeps
    /// retrying with backoff instead of giving up for good; only a session
    /// the server rejected, or one with no stored token, yields `nil`.
    func accessTokenForReconnect() async -> String? {
        let context = captureContext()
        do {
            return try await refreshAccessToken()
        } catch AuthError.refreshUnauthorized {
            if let context { await endSession(context: context) }
            return nil
        } catch AuthError.staleIdentity {
            return nil
        } catch {
            return getAccessToken()
        }
    }

}

private extension AuthManager {
    /// Directly calls the refresh endpoint using URLSession.
    /// We intentionally bypass `APIClient` here to avoid a circular dependency.
    ///
    /// Throws:
    /// - `.noServerUrl` if no base URL is persisted.
    /// - `.refreshUnauthorized` if the server returned 401 (refresh token revoked
    ///    or expired). The caller MUST treat this as a permanent failure.
    /// - `.refreshNetworkFailure` for transport errors, timeouts, or 5xx — the
    ///    caller SHOULD retry rather than logging the user out.
    func refreshTokens(refreshToken: String) async throws -> MobileTokenResponse {
        guard let serverUrl else {
            throw AuthError.noServerUrl
        }
        let generation = authenticationGeneration
        let accountId = user?.id
        let installationId = InstallationID.getOrCreate()
        // A successful Server rotation retains the consumed secret's hash for
        // deletion only, even if an older client discarded the login proof.
        // Persist it before networking, including when the response is lost.
        if KeychainService.loadString(for: KeychainService.revocationTokenKey) == nil {
            try KeychainService.saveString(refreshToken, for: KeychainService.revocationTokenKey)
        }

        guard let url = URL(string: "\(serverUrl)/api/mobile/auth/refresh") else {
            throw AuthError.noServerUrl
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let body = MobileRefreshRequest(
            refreshToken: refreshToken,
            installationId: installationId
        )
        request.httpBody = try JSONEncoder.snakeCase.encode(body)

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await URLSession.shared.data(for: request)
        } catch {
            guard authenticationGeneration == generation else { throw AuthError.staleIdentity }
            throw AuthError.refreshNetworkFailure(error)
        }

        guard authenticationGeneration == generation, self.serverUrl == serverUrl,
              user?.id == accountId, InstallationID.getOrCreate() == installationId else {
            throw AuthError.staleIdentity
        }
        guard let httpResponse = response as? HTTPURLResponse else {
            throw AuthError.refreshNetworkFailure(nil)
        }

        switch httpResponse.statusCode {
        case 200:
            do {
                let apiResponse = try JSONDecoder.snakeCase.decode(
                    ApiResponse<MobileTokenResponse>.self,
                    from: data
                )
                if let accountId, apiResponse.data.user.id != accountId {
                    throw AuthError.staleIdentity
                }
                return apiResponse.data
            } catch AuthError.staleIdentity {
                throw AuthError.staleIdentity
            } catch {
                // Server replied 200 but body did not decode — treat as transient.
                throw AuthError.refreshNetworkFailure(error)
            }
        case 401, 403:
            throw AuthError.refreshUnauthorized
        default:
            // 5xx, 408, 429, anything else — let the caller retry.
            throw AuthError.refreshNetworkFailure(nil)
        }
    }
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

    var errorDescription: String? {
        switch self {
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
}
