import Foundation

/// HTTP client for the ServerBee REST API.
/// Every request and retry belongs to one captured login. Permanent rejection
/// revokes that original session before its local credentials are discarded.
actor APIClient {
    private let authManager: AuthManager

    init(authManager: AuthManager) {
        self.authManager = authManager
    }

    // MARK: - Public API

    func get<T: Decodable & Sendable>(_ path: String) async throws -> T {
        try await request(path, method: "GET")
    }

    func post<T: Decodable & Sendable>(_ path: String, body: (any Encodable & Sendable)? = nil) async throws -> T {
        try await request(path, method: "POST", body: body)
    }

    func put<T: Decodable & Sendable>(_ path: String, body: (any Encodable & Sendable)? = nil) async throws -> T {
        try await request(path, method: "PUT", body: body)
    }

    func delete<T: Decodable & Sendable>(_ path: String) async throws -> T {
        try await request(path, method: "DELETE")
    }

    /// Perform a POST for endpoints with null/empty data.
    func postVoid(_ path: String, body: (any Encodable & Sendable)? = nil) async throws {
        let context = try await currentContext()
        _ = try await authenticatedResponse(path, method: "POST", body: body, context: context)
    }

    /// Preserve error data for DELETE endpoints with null/empty data.
    func deleteVoid(_ path: String) async throws {
        let context = try await currentContext()
        _ = try await authenticatedResponse(path, method: "DELETE", context: context)
    }

    @MainActor
    func captureContext() -> MobileAuthenticationContext? {
        authManager.captureContext()
    }

    @MainActor
    func isCurrent(_ context: MobileAuthenticationContext) -> Bool {
        authManager.isCurrent(context)
    }

    func postVoid(
        _ path: String,
        body: (any Encodable & Sendable)? = nil,
        context: MobileAuthenticationContext
    ) async throws {
        _ = try await authenticatedResponse(path, method: "POST", body: body, context: context)
    }

    func get<T: Decodable & Sendable>(_ path: String, context: MobileAuthenticationContext) async throws -> T {
        try await response(path, method: "GET", context: context)
    }

    func send<T: Decodable & Sendable>(
        _ path: String, method: String, body: any Encodable & Sendable, context: MobileAuthenticationContext
    ) async throws -> T {
        try await response(path, method: method, body: body, context: context)
    }

    private func response<T: Decodable & Sendable>(
        _ path: String, method: String, body: (any Encodable & Sendable)? = nil, context: MobileAuthenticationContext
    ) async throws -> T {
        let (data, _) = try await authenticatedResponse(path, method: method, body: body, context: context)
        return try JSONDecoder.snakeCase.decode(ApiResponse<T>.self, from: data).data
    }

    /// Cleanup may finish after a login change, but always targets the captured
    /// deployment and credential. Only the original active login may refresh.
    func postCleanup(_ path: String, context: MobileAuthenticationContext) async throws {
        let token = await authManager.accessToken(ifCurrent: context) ?? context.accessToken
        var (_, response) = try await sendRequest(path, context: context, token: token)
        if response.statusCode == 401, await authManager.isCurrent(context) {
            // Unregister is part of explicit logout. Its owner revokes the
            // session next, so retain the proof even if this cleanup is denied.
            let rotated = try await authManager.refreshAccessToken(context: context)
            guard await authManager.isCurrent(context) else { throw AuthError.staleIdentity }
            (_, response) = try await sendRequest(path, context: context, token: rotated)
        }
        guard (200...299).contains(response.statusCode) else {
            throw APIError.httpError(statusCode: response.statusCode, data: Data())
        }
    }

    /// Try only proofs captured from the original login. A saved stable proof
    /// may be stale after iOS-first upgrades against a Server that replaced
    /// sessions on refresh. Its captured refresh secret can delete that session
    /// before consumption or through retained history after a lost response.
    func revokeSession(context capturedContext: MobileAuthenticationContext) async throws {
        let context = await cleanupContext(capturedContext)
        var credentials = [String]()
        for candidate in [context.revocationToken, context.refreshToken] {
            if let candidate, !credentials.contains(candidate) { credentials.append(candidate) }
        }
        for credential in credentials {
            let (_, response) = try await sendRequest(
                "/api/mobile/auth/revoke",
                body: MobileRevokeRequest(installationId: context.installationId, revocationToken: credential),
                context: context, token: nil
            )
            if (200...299).contains(response.statusCode) { return }
            // A missing endpoint belongs to an older Server; additional proofs
            // cannot help. A rejected proof may be stale, so try the original
            // captured secret next, without refreshing or adopting a new login.
            if response.statusCode == 404 { break }
            guard response.statusCode == 401 else {
                throw APIError.httpError(statusCode: response.statusCode, data: Data())
            }
        }
        let token = await authManager.accessToken(ifCurrent: context) ?? context.accessToken
        let (_, response) = try await sendRequest("/api/mobile/auth/logout", context: context, token: token)
        guard (200...299).contains(response.statusCode) else {
            throw APIError.httpError(statusCode: response.statusCode, data: Data())
        }
    }

    private static let cleanupSession: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.httpCookieStorage = nil
        configuration.httpShouldSetCookies = false
        configuration.urlCredentialStorage = nil
        configuration.urlCache = nil
        return URLSession(configuration: configuration)
    }()

    /// Replay has no AuthManager dependency and cannot refresh or adopt a login.
    static func revokeSavedSession(_ record: PendingSessionRevocation) async throws {
        guard let url = URL(string: "\(record.serverUrl)/api/mobile/auth/revoke") else { throw APIError.noServerUrl }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 12
        request.httpShouldHandleCookies = false
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder.snakeCase.encode(MobileRevokeRequest(
            installationId: record.installationId, revocationToken: record.proof, expectedSessionId: record.mobileSessionId))
        let (data, response) = try await ServerHTTPTransport.data(for: request, session: cleanupSession)
        guard let response = response as? HTTPURLResponse, response.statusCode == 200 else {
            throw APIError.httpError(statusCode: (response as? HTTPURLResponse)?.statusCode ?? -1, data: data)
        }
        // A malformed 200 is not evidence that the intended endpoint committed.
        let acknowledgement = try JSONDecoder.snakeCase.decode(ApiResponse<String>.self, from: data).data
        guard acknowledgement == "ok" || acknowledgement == "already_absent" else {
            throw APIError.httpError(statusCode: 200, data: data)
        }
    }

    func requireDeletionRecovery(context: MobileAuthenticationContext) async throws {
        try await authManager.requireDeletionRecovery(context: context)
    }

    // MARK: - Captured requests

    @MainActor
    private func currentContext() throws -> MobileAuthenticationContext {
        guard authManager.serverUrl != nil else { throw APIError.noServerUrl }
        guard let context = authManager.captureContext() else { throw APIError.unauthorized }
        return context
    }

    /// Refresh credentials only at request entry, atomically with the complete
    /// captured identity check. Long-lived push contexts can predate baseline
    /// session replacements; a replacement login is never a credential source.
    @MainActor
    private func currentContext(matching captured: MobileAuthenticationContext) throws -> MobileAuthenticationContext {
        guard authManager.isCurrent(captured), let current = authManager.captureContext() else { throw AuthError.staleIdentity }
        return current
    }

    @MainActor
    private func cleanupContext(_ captured: MobileAuthenticationContext) -> MobileAuthenticationContext {
        guard authManager.isCurrent(captured), let current = authManager.captureContext() else { return captured }
        return current
    }

    private func request<T: Decodable & Sendable>(
        _ path: String,
        method: String,
        body: (any Encodable & Sendable)? = nil
    ) async throws -> T {
        let context = try await currentContext()
        let (data, _) = try await authenticatedResponse(path, method: method, body: body, context: context)
        do {
            return try JSONDecoder.snakeCase.decode(ApiResponse<T>.self, from: data).data
        } catch {
            throw APIError.decodingError(error)
        }
    }

    /// All ordinary API entry points share identity checks and expiry cleanup.
    private func authenticatedResponse(
        _ path: String,
        method: String,
        body: (any Encodable & Sendable)? = nil,
        context capturedContext: MobileAuthenticationContext
    ) async throws -> (Data, HTTPURLResponse) {
        if (path == "/api/mobile/push/settings" && method == "PUT") || path == "/api/mobile/push/verified-register" {
            try await authManager.requireDeletionRecovery(context: capturedContext)
        }
        let context = try await currentContext(matching: capturedContext)
        var token = context.accessToken
        var result = try await sendRequest(path, method: method, body: body, context: context, token: token)
        guard await authManager.isCurrent(context) else { throw AuthError.staleIdentity }
        if result.1.statusCode == 401 {
            do {
                token = try await authManager.refreshAccessToken(context: context)
            } catch AuthError.refreshUnauthorized {
                await authManager.endSession(context: context)
                throw APIError.unauthorized
            } catch AuthError.staleIdentity {
                throw AuthError.staleIdentity
            } catch {
                // A transport failure preserves the original credentials/proof.
                throw APIError.network(error)
            }
            guard await authManager.isCurrent(context) else { throw AuthError.staleIdentity }
            result = try await sendRequest(path, method: method, body: body, context: context, token: token)
            guard await authManager.isCurrent(context) else { throw AuthError.staleIdentity }
            if result.1.statusCode == 401 {
                await authManager.endSession(context: context)
                throw APIError.unauthorized
            }
        }
        guard (200...299).contains(result.1.statusCode) else {
            throw APIError.httpError(statusCode: result.1.statusCode, data: result.0)
        }
        return result
    }

    private func sendRequest(
        _ path: String,
        method: String = "POST",
        body: (any Encodable & Sendable)? = nil,
        context: MobileAuthenticationContext,
        token: String?
    ) async throws -> (Data, HTTPURLResponse) {
        guard let url = URL(string: "\(context.serverUrl)\(path)") else { throw APIError.noServerUrl }
        // Enforce this at the transport entry, so every registration caller and
        // its post-refresh retry has the same secure content-key boundary.
        if path == "/api/mobile/push/verified-register", url.scheme != "https" {
            throw PushSetupError.insecureServer
        }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        if let token { request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization") }
        if let body { request.httpBody = try JSONEncoder.snakeCase.encode(body) }
        let (data, response) = try await ServerHTTPTransport.data(for: request)
        guard let response = response as? HTTPURLResponse else {
            throw APIError.httpError(statusCode: -1, data: data)
        }
        return (data, response)
    }
}

// MARK: - API Errors

enum APIError: Error, LocalizedError {
    case noServerUrl
    case unauthorized
    case network(Error)
    case httpError(statusCode: Int, data: Data)
    case decodingError(Error)

    var errorDescription: String? {
        switch self {
        case .noServerUrl:
            return String(localized: "No server URL configured")
        case .unauthorized:
            return String(localized: "Session expired. Please log in again.")
        case .network(let error):
            return String(localized: "Network error: \(error.localizedDescription)")
        case .httpError(let statusCode, _):
            return String(localized: "Server returned HTTP \(statusCode)")
        case .decodingError(let error):
            return String(localized: "Failed to decode response: \(error.localizedDescription)")
        }
    }
}
