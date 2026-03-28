import Foundation
import SwiftUI

/// Wraps all API responses: `{ "data": T }`
private struct APIResponse<T: Decodable & Sendable>: Decodable, Sendable {
    let data: T
}

/// Represents an error response from the API.
struct APIErrorResponse: Decodable, Sendable {
    let code: String?
    let message: String?
}

enum APIError: Error, Sendable {
    case invalidURL
    case httpError(statusCode: Int, body: APIErrorResponse?)
    case decodingError(Error)
    case networkError(Error)
}

// MARK: - Environment Key

private struct APIClientKey: EnvironmentKey {
    static let defaultValue: APIClient = APIClient(baseURL: "")
}

extension EnvironmentValues {
    var apiClient: APIClient {
        get { self[APIClientKey.self] }
        set { self[APIClientKey.self] = newValue }
    }
}

// MARK: - APIClient

actor APIClient {
    let baseURL: String
    private var accessToken: String?

    private let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.keyEncodingStrategy = .convertToSnakeCase
        return e
    }()

    private let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }()

    init(baseURL: String) {
        self.baseURL = baseURL
    }

    func setAccessToken(_ token: String?) {
        self.accessToken = token
    }

    func get<T: Decodable & Sendable>(_ path: String) async throws -> T {
        let request = try buildRequest(method: "GET", path: path)
        return try await perform(request)
    }

    func post<T: Decodable & Sendable, B: Encodable & Sendable>(
        _ path: String,
        body: B
    ) async throws -> T {
        let request = try buildRequest(method: "POST", path: path, body: body)
        return try await perform(request)
    }

    /// POST that returns no meaningful body.
    func postVoid<B: Encodable & Sendable>(
        _ path: String,
        body: B
    ) async throws {
        let request = try buildRequest(method: "POST", path: path, body: body)
        try await send(request)
    }

    // MARK: - Private

    private func buildRequest<B: Encodable & Sendable>(
        method: String,
        path: String,
        body: B? = nil as String?
    ) throws -> URLRequest {
        guard let url = URL(string: baseURL + path) else {
            throw APIError.invalidURL
        }
        var request = URLRequest(url: url)
        request.httpMethod = method
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try encoder.encode(body)
        }
        if let token = accessToken {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        return request
    }

    /// Execute a request and validate the HTTP status, returning raw response data.
    @discardableResult
    private func send(_ request: URLRequest) async throws -> Data {
        let data: Data
        let response: URLResponse

        do {
            (data, response) = try await URLSession.shared.data(for: request)
        } catch {
            throw APIError.networkError(error)
        }

        guard let httpResponse = response as? HTTPURLResponse else {
            throw APIError.networkError(
                NSError(domain: "APIClient", code: -1, userInfo: [NSLocalizedDescriptionKey: "Invalid response"])
            )
        }

        guard (200...299).contains(httpResponse.statusCode) else {
            let errorBody = try? decoder.decode(APIErrorResponse.self, from: data)
            throw APIError.httpError(statusCode: httpResponse.statusCode, body: errorBody)
        }

        return data
    }

    /// Execute a request, validate status, and decode the `{ "data": T }` wrapper.
    private func perform<T: Decodable & Sendable>(_ request: URLRequest) async throws -> T {
        let data = try await send(request)
        do {
            let wrapped = try decoder.decode(APIResponse<T>.self, from: data)
            return wrapped.data
        } catch {
            throw APIError.decodingError(error)
        }
    }
}
