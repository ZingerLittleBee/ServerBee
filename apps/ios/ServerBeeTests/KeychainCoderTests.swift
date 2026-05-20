import XCTest
@testable import ServerBee

final class KeychainCoderTests: XCTestCase {
    struct Sample: Codable, Equatable {
        let myField: String
        let otherValue: Int
    }

    func testRoundTripUsesSnakeCase() throws {
        let original = Sample(myField: "hello", otherValue: 42)
        let key = "test_keychain_coder_roundtrip"
        defer { KeychainService.delete(for: key) }

        try KeychainService.saveCodable(original, for: key)
        let raw = KeychainService.load(for: key)!
        let asString = String(data: raw, encoding: .utf8)!
        XCTAssertTrue(asString.contains("my_field"), "Encoded payload should use snake_case keys, got: \(asString)")

        let decoded: Sample? = KeychainService.loadCodable(for: key)
        XCTAssertEqual(decoded, original)
    }
}
