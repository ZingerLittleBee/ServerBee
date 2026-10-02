import Foundation

/// HTTP client for the ServerBee REST API.
///
/// Automatically attaches the Bearer access token to every request and
/// handles 401 responses by attempting a single token refresh before
/// retrying. On a second failure the user is logged out.
actor APIClient {
    private let authManager: AuthManager

    init(authManager: AuthManager) {
        self.authManager = authManager
    }

    // MARK: - Public API

    /// Perform a GET request and decode the response.
    func get<T: Decodable & Sendable>(_ path: String) async throws -> T {
        try await request(path, method: "GET")
    }

    /// Perform a POST request with an optional JSON body and decode the response.
    func post<T: Decodable & Sendable>(_ path: String, body: (any Encodable & Sendable)? = nil) async throws -> T {
        try await request(path, method: "POST", body: body)
    }

    /// Perform a POST request for endpoints that return null/empty data.
    func postVoid(_ path: String, body: (any Encodable & Sendable)? = nil) async throws {
        let (_, httpResponse) = try await performRequest(path, method: "POST", body: body)

        if httpResponse.statusCode == 401 {
            try await refreshOrThrow()
            let (_, retryResponse) = try await performRequest(path, method: "POST", body: body)
            if retryResponse.statusCode == 401 {
                await authManager.clearAuth()
                throw APIError.unauthorized
            }
            guard (200...299).contains(retryResponse.statusCode) else {
                throw APIError.httpError(statusCode: retryResponse.statusCode, data: Data())
            }
            return
        }

        guard (200...299).contains(httpResponse.statusCode) else {
            throw APIError.httpError(statusCode: httpResponse.statusCode, data: Data())
        }
    }

    /// Perform a PUT request with an optional JSON body and decode the response.
    func put<T: Decodable & Sendable>(_ path: String, body: (any Encodable & Sendable)? = nil) async throws -> T {
        try await request(path, method: "PUT", body: body)
    }

    /// Perform a DELETE request and decode the response.
    func delete<T: Decodable & Sendable>(_ path: String) async throws -> T {
        try await request(path, method: "DELETE")
    }

    /// Perform a DELETE request for endpoints that return `{ "data": null }`
    /// (which `delete<T>` can't decode). Preserves the response body in the
    /// thrown error so callers can surface the server's message.
    func deleteVoid(_ path: String) async throws {
        var (data, httpResponse) = try await performRequest(path, method: "DELETE")
        if httpResponse.statusCode == 401 {
            try await refreshOrThrow()
            (data, httpResponse) = try await performRequest(path, method: "DELETE")
            if httpResponse.statusCode == 401 {
                await authManager.clearAuth()
                throw APIError.unauthorized
            }
        }
        guard (200...299).contains(httpResponse.statusCode) else {
            throw APIError.httpError(statusCode: httpResponse.statusCode, data: data)
        }
    }

    /// Registration and logout requests use one captured login. A stale 401
    /// must never refresh or retry using a replacement account/deployment.
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
        guard var token = await authManager.accessToken(ifCurrent: context) else { throw AuthError.staleIdentity }
        var response = try await performCapturedRequest(path, body: body, context: context, token: token)
        guard await authManager.isCurrent(context) else { throw AuthError.staleIdentity }
        if response.statusCode == 401 {
            do {
                token = try await authManager.refreshAccessToken(context: context)
            } catch AuthError.refreshUnauthorized {
                await authManager.clearAuth(ifCurrent: context)
                throw APIError.unauthorized
            }
            guard await authManager.isCurrent(context) else { throw AuthError.staleIdentity }
            response = try await performCapturedRequest(path, body: body, context: context, token: token)
            guard await authManager.isCurrent(context) else { throw AuthError.staleIdentity }
            if response.statusCode == 401 {
                await authManager.clearAuth(ifCurrent: context)
                throw APIError.unauthorized
            }
        }
        guard (200...299).contains(response.statusCode) else {
            throw APIError.httpError(statusCode: response.statusCode, data: Data())
        }
    }

    /// Cleanup may finish after a login change, but always targets the captured
    /// deployment and credential. Only the original active login may refresh.
    func postCleanup(_ path: String, context: MobileAuthenticationContext) async throws {
        let token = await authManager.accessToken(ifCurrent: context) ?? context.accessToken
        let response = try await performCapturedRequest(path, body: nil, context: context, token: token)
        if response.statusCode == 401, await authManager.isCurrent(context) {
            try await postVoid(path, context: context)
            return
        }
        guard (200...299).contains(response.statusCode) else {
            throw APIError.httpError(statusCode: response.statusCode, data: Data())
        }
    }

    /// A stable deletion-only proof survives committed rotations with lost
    /// responses. It never authenticates requests or refreshes a replacement login.
    func revokeSession(context: MobileAuthenticationContext) async throws {
        if let credential = context.revocationToken {
            let response = try await performCapturedRequest(
                "/api/mobile/auth/revoke",
                body: MobileRevokeRequest(installationId: context.installationId, revocationToken: credential),
                context: context, token: nil
            )
            if (200...299).contains(response.statusCode) { return }
            // Older Servers and not-yet-upgraded legacy sessions still support
            // ordinary authenticated logout. Neither fallback grants old tokens API access.
            guard response.statusCode == 404 || response.statusCode == 401 else {
                throw APIError.httpError(statusCode: response.statusCode, data: Data())
            }
        }
        try await postCleanup("/api/mobile/auth/logout", context: context)
    }

    private func performCapturedRequest(
        _ path: String,
        body: (any Encodable & Sendable)?,
        context: MobileAuthenticationContext,
        token: String?
    ) async throws -> HTTPURLResponse {
        guard let url = URL(string: "\(context.serverUrl)\(path)") else { throw APIError.noServerUrl }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        if let token { request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization") }
        if let body { request.httpBody = try JSONEncoder.snakeCase.encode(body) }
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let response = response as? HTTPURLResponse else {
            throw APIError.httpError(statusCode: -1, data: data)
        }
        return response
    }

    // MARK: - Internal

    private func request<T: Decodable & Sendable>(
        _ path: String,
        method: String,
        body: (any Encodable & Sendable)? = nil
    ) async throws -> T {
        let (data, httpResponse) = try await performRequest(path, method: method, body: body)

        if httpResponse.statusCode == 401 {
            return try await handleUnauthorized(path: path, method: method, body: body)
        }

        guard (200...299).contains(httpResponse.statusCode) else {
            throw APIError.httpError(statusCode: httpResponse.statusCode, data: data)
        }

        do {
            let wrapper = try JSONDecoder.snakeCase.decode(ApiResponse<T>.self, from: data)
            return wrapper.data
        } catch {
            throw APIError.decodingError(error)
        }
    }

    /// Build and fire a single URLRequest. Returns the raw data + HTTP response.
    private func performRequest(
        _ path: String,
        method: String,
        body: (any Encodable & Sendable)? = nil
    ) async throws -> (Data, HTTPURLResponse) {
        // AuthManager is @MainActor-isolated; hop to read state.
        let serverUrl = await authManager.serverUrl
        let token = await authManager.getAccessToken()

        guard let serverUrl else {
            throw APIError.noServerUrl
        }
        guard let url = URL(string: "\(serverUrl)\(path)") else {
            throw APIError.noServerUrl
        }

        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        // Attach bearer token if available
        if let token {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }

        if let body {
            request.httpBody = try JSONEncoder.snakeCase.encode(body)
        }

        let (data, response) = try await URLSession.shared.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw APIError.httpError(statusCode: -1, data: data)
        }

        return (data, httpResponse)
    }

    // MARK: - 401 Handling

    private func handleUnauthorized<T: Decodable & Sendable>(
        path: String,
        method: String,
        body: (any Encodable & Sendable)?
    ) async throws -> T {
        try await refreshOrThrow()

        let (data, httpResponse) = try await performRequest(path, method: method, body: body)

        if httpResponse.statusCode == 401 {
            // Refresh succeeded but server still rejects — credentials definitely revoked.
            await authManager.clearAuth()
            throw APIError.unauthorized
        }

        guard (200...299).contains(httpResponse.statusCode) else {
            throw APIError.httpError(statusCode: httpResponse.statusCode, data: data)
        }

        do {
            let wrapper = try JSONDecoder.snakeCase.decode(ApiResponse<T>.self, from: data)
            return wrapper.data
        } catch {
            throw APIError.decodingError(error)
        }
    }

    /// Run a refresh; classify the failure mode.
    ///
    /// - On `.refreshUnauthorized`: clear local auth and surface `.unauthorized`.
    /// - On `.refreshNetworkFailure`: leave local auth intact and surface
    ///   `.network` so the caller can show a transient error instead of
    ///   kicking the user back to the login screen.
    private func refreshOrThrow() async throws {
        do {
            _ = try await authManager.refreshAccessToken()
        } catch AuthError.refreshUnauthorized {
            await authManager.clearAuth()
            throw APIError.unauthorized
        } catch {
            // .refreshNetworkFailure, .noServerUrl, or anything else transient.
            throw APIError.network(error)
        }
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
