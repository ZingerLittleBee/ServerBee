import XCTest
@testable import ServerBee

/// Decoding + encoding coverage for M7 account/security models, matching the
/// live `/api/auth/*`, `/api/mobile/auth/devices`, and `/api/{geoip,asn}/status`
/// payloads.
final class AccountModelsDecodingTests: XCTestCase {
    private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
        try JSONDecoder.snakeCase.decode(T.self, from: Data(json.utf8))
    }

    // MARK: - API keys

    func test_apiKey_listItem_hasNullKey() throws {
        let json = """
        { "id": "4df2b3ee", "name": "dev", "key_prefix": "wHlTvf6A",
          "created_at": "2026-04-11T17:45:40.563500125+00:00", "key": null }
        """
        let key = try decode(ApiKey.self, json)
        XCTAssertEqual(key.name, "dev")
        XCTAssertEqual(key.keyPrefix, "wHlTvf6A")
        XCTAssertNil(key.key)
    }

    func test_apiKey_createResponse_carriesPlaintextOnce() throws {
        let json = """
        { "id": "k1", "name": "ci", "key_prefix": "AbCdEfGh",
          "created_at": "2026-06-01T00:00:00Z", "key": "serverbee_AbCdEfGh0123456789" }
        """
        let key = try decode(ApiKey.self, json)
        XCTAssertEqual(key.key, "serverbee_AbCdEfGh0123456789")
    }

    func test_createApiKeyRequest_encodes() throws {
        let data = try JSONEncoder.snakeCase.encode(CreateApiKeyRequest(name: "ci"))
        XCTAssertTrue(String(data: data, encoding: .utf8)!.contains("\"name\":\"ci\""))
    }

    // MARK: - Devices

    func test_mobileDevice_decodes() throws {
        let json = """
        { "id": "4711e5d2", "device_name": "iPhone 16", "installation_id": "inst-1",
          "created_at": "2026-06-14T16:16:01.286120720+00:00",
          "last_used_at": "2026-06-14T16:16:02.355096141+00:00" }
        """
        let d = try decode(MobileDevice.self, json)
        XCTAssertEqual(d.deviceName, "iPhone 16")
        XCTAssertEqual(d.installationId, "inst-1")
    }

    // MARK: - 2FA

    func test_twoFactorStatus_decodes() throws {
        XCTAssertTrue(try decode(TwoFactorStatus.self, #"{"enabled":true}"#).enabled)
        XCTAssertFalse(try decode(TwoFactorStatus.self, #"{"enabled":false}"#).enabled)
    }

    func test_twoFactorSetup_decodes() throws {
        let json = #"{"secret":"XM6VT3","otpauth_url":"otpauth://totp/x","qr_code_base64":"AAAA"}"#
        let s = try decode(TwoFactorSetup.self, json)
        XCTAssertEqual(s.secret, "XM6VT3")
        XCTAssertEqual(s.otpauthUrl, "otpauth://totp/x")
        XCTAssertEqual(s.qrCodeBase64, "AAAA")
    }

    func test_changePasswordRequest_encodesSnakeCase() throws {
        let data = try JSONEncoder.snakeCase.encode(ChangePasswordRequest(oldPassword: "a", newPassword: "bbbbbbbb"))
        let json = String(data: data, encoding: .utf8)!
        XCTAssertTrue(json.contains("\"old_password\":\"a\""))
        XCTAssertTrue(json.contains("\"new_password\":\"bbbbbbbb\""))
    }

    // MARK: - About / DB status

    func test_aboutInfo_decodes() throws {
        XCTAssertEqual(try decode(AboutInfo.self, #"{"version":"1.2.3"}"#).version, "1.2.3")
    }

    func test_dbStatus_decodesInstalled() throws {
        let json = """
        { "installed": true, "source": "downloaded", "file_size": 8272525,
          "updated_at": "2026-05-21T15:21:10.370847576+00:00" }
        """
        let s = try decode(DbStatus.self, json)
        XCTAssertTrue(s.installed)
        XCTAssertEqual(s.source, "downloaded")
        XCTAssertEqual(s.fileSize, 8_272_525)
    }

    func test_dbStatus_decodesNotInstalled() throws {
        let s = try decode(DbStatus.self, #"{"installed":false,"source":null,"file_size":null,"updated_at":null}"#)
        XCTAssertFalse(s.installed)
        XCTAssertNil(s.fileSize)
    }
}
