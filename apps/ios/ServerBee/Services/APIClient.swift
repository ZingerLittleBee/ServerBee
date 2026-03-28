import Foundation

/// HTTP client for the ServerBee REST API.
///
/// Automatically attaches the Bearer access token to every request and
/// handles 401 responses by attempting a single token refresh before
/// retrying. On a second failure the user is logged out.
actor APIClient {
    private let authManager: AuthManager
    private var isRefreshing = false
    private var refreshContinuations: [CheckedContinuation<MobileTokenResponse, Error>] = []

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
        let _: ApiResponse<Empty?> = try await request(path, method: "POST", body: body)
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

        // On 401: attempt one token refresh, then retry.
        if httpResponse.statusCode == 401 {
            return try await handleUnauthorized(path: path, method: method, body: body)
        }

        guard (200...299).contains(httpResponse.statusCode) else {
            throw APIError.httpError(statusCode: httpResponse.statusCode, data: data)
        }

        do {
            return try JSONDecoder.snakeCase.decode(T.self, from: data)
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
        guard let serverUrl = await authManager.serverUrl else {
            throw APIError.noServerUrl
        }
        guard let url = URL(string: "\(serverUrl)\(path)") else {
            throw APIError.noServerUrl
        }

        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        // Attach bearer token if available
        if let token = await authManager.getAccessToken() {
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

    // MARK: - 401 Handling with Coalesced Refresh

    /// When a 401 is received, attempt to refresh the token exactly once.
    /// Concurrent 401s are coalesced so only one refresh request is made.
    private func handleUnauthorized<T: Decodable & Sendable>(
        path: String,
        method: String,
        body: (any Encodable & Sendable)?
    ) async throws -> T {
        // Attempt to refresh
        do {
            try await performTokenRefresh()
        } catch {
            await authManager.clearAuth()
            throw APIError.unauthorized
        }

        // Retry the original request with the new token
        let (data, httpResponse) = try await performRequest(path, method: method, body: body)

        // If still 401 after refresh, give up
        if httpResponse.statusCode == 401 {
            await authManager.clearAuth()
            throw APIError.unauthorized
        }

        guard (200...299).contains(httpResponse.statusCode) else {
            throw APIError.httpError(statusCode: httpResponse.statusCode, data: data)
        }

        do {
            return try JSONDecoder.snakeCase.decode(T.self, from: data)
        } catch {
            throw APIError.decodingError(error)
        }
    }

    /// Perform a single token refresh, coalescing concurrent callers.
    private func performTokenRefresh() async throws {
        // If a refresh is already in-flight, wait for its result
        if isRefreshing {
            _ = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<MobileTokenResponse, Error>) in
                refreshContinuations.append(continuation)
            }
            return
        }

        isRefreshing = true

        do {
            guard let refreshToken = KeychainService.loadString(for: KeychainService.refreshTokenKey) else {
                let error = APIError.unauthorized
                resumeWaiters(with: .failure(error))
                isRefreshing = false
                throw error
            }

            let tokenResponse = try await callRefreshEndpoint(refreshToken: refreshToken)
            await authManager.handleLoginResponse(tokenResponse)
            resumeWaiters(with: .success(tokenResponse))
            isRefreshing = false
        } catch {
            resumeWaiters(with: .failure(error))
            isRefreshing = false
            throw error
        }
    }

    /// Resume all queued continuations waiting on the refresh result.
    private func resumeWaiters(with result: Result<MobileTokenResponse, Error>) {
        let waiters = refreshContinuations
        refreshContinuations = []
        for continuation in waiters {
            continuation.resume(with: result)
        }
    }

    /// Direct call to the refresh endpoint (bypasses `request` to avoid recursion).
    private func callRefreshEndpoint(refreshToken: String) async throws -> MobileTokenResponse {
        guard let serverUrl = await authManager.serverUrl else {
            throw APIError.noServerUrl
        }
        guard let url = URL(string: "\(serverUrl)/api/mobile/auth/refresh") else {
            throw APIError.noServerUrl
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let body = MobileRefreshRequest(
            refreshToken: refreshToken,
            installationId: InstallationID.getOrCreate()
        )
        request.httpBody = try JSONEncoder.snakeCase.encode(body)

        let (data, response) = try await URLSession.shared.data(for: request)

        guard let httpResponse = response as? HTTPURLResponse, httpResponse.statusCode == 200 else {
            throw APIError.unauthorized
        }

        return try JSONDecoder.snakeCase.decode(ApiResponse<MobileTokenResponse>.self, from: data).data
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
