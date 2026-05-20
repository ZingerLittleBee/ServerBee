import Foundation

// MARK: - Generic API Response Wrapper

struct ApiResponse<T: Decodable & Sendable>: Decodable, Sendable {
    let data: T
}

// MARK: - Empty Response

/// Used for API endpoints that return null/empty data.
struct Empty: Codable, Sendable {}

// MARK: - JSON Coding Helpers

extension JSONEncoder {
    /// Encoder that relies on explicit `CodingKeys` in each model.
    ///
    /// Historically this set `.keyEncodingStrategy = .convertToSnakeCase`,
    /// which conflicted with the hand-written `CodingKeys` already on every
    /// model and risked double-conversion if a property's CodingKey was
    /// itself camelCase. See `Models/README.md`.
    static let snakeCase: JSONEncoder = JSONEncoder()
}

extension JSONDecoder {
    /// Decoder that relies on explicit `CodingKeys` in each model.
    static let snakeCase: JSONDecoder = JSONDecoder()
}
