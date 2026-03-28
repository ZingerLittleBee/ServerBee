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

        var urlRequest = URLRequest(url: url)
        urlRequest.httpMethod = method
        urlRequest.setValue("application/json", forHTTPHeaderField: "Content-Type")

        if let token = authManager.getAccessToken() {
            urlRequest.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }

        if let body {
            urlRequest.httpBody = try JSONEncoder.snakeCase.encode(body)
        }

        let (data, response) = try await URLSession.shared.data(for: urlRequest)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw APIError.httpError(statusCode: -1, data: data)
        }

        return (data, httpResponse)
    }

    // MARK: - 401 Handling with Coalesced Refresh

    private func handleUnauthorized<T: Decodable & Sendable>(
        path: String,
        method: String,
        body: (any Encodable & Sendable)?
    ) async throws -> T {
        do {
            try await performTokenRefresh()
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
            return try JSONDecoder.snakeCase.decode(T.self, from: data)
        } catch {
            throw APIError.decodingError(error)
        }
    }

    private func performTokenRefresh() async throws {
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

    private func resumeWaiters(with result: Result<MobileTokenResponse, Error>) {
        let waiters = refreshContinuations
        refreshContinuations = []
        for continuation in waiters {
            continuation.resume(with: result)
        }
    }

    private func callRefreshEndpoint(refreshToken: String) async throws -> MobileTokenResponse {
        guard let serverUrl = authManager.serverUrl else {
            throw APIError.noServerUrl
        }
        guard let url = URL(string: "\(serverUrl)/api/mobile/auth/refresh") else {
            throw APIError.noServerUrl
        }

        var urlRequest = URLRequest(url: url)
        urlRequest.httpMethod = "POST"
        urlRequest.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let body = MobileRefreshRequest(
            refreshToken: refreshToken,
            installationId: InstallationID.getOrCreate()
        )
        urlRequest.httpBody = try JSONEncoder.snakeCase.encode(body)

        let (data, response) = try await URLSession.shared.data(for: urlRequest)

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

// MARK: - Empty Response

struct Empty: Codable, Sendable {}
