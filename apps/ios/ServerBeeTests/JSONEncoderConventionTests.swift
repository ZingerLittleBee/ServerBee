import XCTest
@testable import ServerBee

final class JSONEncoderConventionTests: XCTestCase {
    func test_loginRequest_encodesViaCodingKeys_notKeyStrategy() throws {
        let req = MobileLoginRequest(
            username: "alice",
            password: "pw",
            installationId: "iid-1",
            deviceName: "iPhone",
            totpCode: nil
        )
        let data = try JSONEncoder.snakeCase.encode(req)
        let json = String(data: data, encoding: .utf8)!
        XCTAssertTrue(json.contains("\"installation_id\":\"iid-1\""),
                      "CodingKeys must produce snake_case for installation_id")
        XCTAssertTrue(json.contains("\"device_name\":\"iPhone\""),
                      "CodingKeys must produce snake_case for device_name")
    }

    /// If `.convertToSnakeCase` were still active AND a model had a property
    /// without a CodingKey override, both transformations could combine and
    /// double-snake or otherwise corrupt the key. Pin a property that *does*
    /// have an override to confirm it round-trips cleanly via CodingKeys alone.
    func test_refreshRequest_encodesRefreshTokenSnakeCase() throws {
        let req = MobileRefreshRequest(refreshToken: "tok", installationId: "iid")
        let data = try JSONEncoder.snakeCase.encode(req)
        let json = String(data: data, encoding: .utf8)!
        XCTAssertTrue(json.contains("\"refresh_token\":\"tok\""))
        XCTAssertFalse(json.contains("\"refreshToken\""))
    }
}
