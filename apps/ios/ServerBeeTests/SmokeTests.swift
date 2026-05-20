import XCTest
@testable import ServerBee

final class SmokeTests: XCTestCase {
    func test_browserMessageDecoder_decodesServerOnline() throws {
        let json = #"{"type":"server_online","server_id":"abc-123"}"#
        let data = Data(json.utf8)
        let message = try JSONDecoder.snakeCase.decode(BrowserMessage.self, from: data)
        if case .serverOnline(let id) = message {
            XCTAssertEqual(id, "abc-123")
        } else {
            XCTFail("Expected .serverOnline")
        }
    }
}
