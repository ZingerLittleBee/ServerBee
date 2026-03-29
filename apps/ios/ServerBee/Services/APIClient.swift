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
            do {
                _ = try await authManager.refreshAccessToken()
            } catch {
                await authManager.clearAuth()
                throw APIError.unauthorized
            }
            let (_, retryResponse) = try await performRequest(path, method: "POST", body: body)
            guard (200...299).contains(retryResponse.statusCode) else {
                await authManager.clearAuth()
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
        guard let serverUrl = authManager.serverUrl else {
            throw APIError.noServerUrl
        }
        guard let url = URL(string: "\(serverUrl)\(path)") else {
            throw APIError.noServerUrl
        }

        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        // Attach bearer token if available
        if let token = authManager.getAccessToken() {
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
        do {
            _ = try await authManager.refreshAccessToken()
        } catch {
            await authManager.clearAuth()
            throw APIError.unauthorized
        }

        let (data, httpResponse) = try await performRequest(path, method: method, body: body)

        if httpResponse.statusCode == 401 {
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
}

// MARK: - API Errors

enum APIError: Error, LocalizedError {
    case noServerUrl
    case unauthorized
    case httpError(statusCode: Int, data: Data)
    case decodingError(Error)

    var errorDescription: String? {
        switch self {
        case .noServerUrl:
            return "No server URL configured"
        case .unauthorized:
            return "Session expired — please log in again"
        case .httpError(let statusCode, _):
            return "Server returned HTTP \(statusCode)"
        case .decodingError(let error):
            return "Failed to decode response: \(error.localizedDescription)"
        }
    }
}
