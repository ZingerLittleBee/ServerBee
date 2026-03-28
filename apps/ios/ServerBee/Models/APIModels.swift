import Foundation

// MARK: - Generic API Response Wrapper

struct ApiResponse<T: Decodable>: Decodable {
    let data: T
}

// MARK: - Empty Response

/// Used for API endpoints that return null/empty data.
struct Empty: Codable, Sendable {}

// MARK: - JSON Coding Helpers

extension JSONEncoder {
    /// Encoder configured for snake_case keys.
    static let snakeCase: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        return encoder
    }()
}

extension JSONDecoder {
    /// Decoder configured for snake_case keys.
    static let snakeCase: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }()
}
