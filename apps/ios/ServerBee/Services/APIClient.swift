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
            return "No server URL configured"
        case .unauthorized:
            return "Session expired — please log in again"
        case .network(let error):
            return "Network error: \(error.localizedDescription)"
        case .httpError(let statusCode, _):
            return "Server returned HTTP \(statusCode)"
        case .decodingError(let error):
            return "Failed to decode response: \(error.localizedDescription)"
        }
    }
}
