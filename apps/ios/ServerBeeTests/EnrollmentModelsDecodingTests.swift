import XCTest
@testable import ServerBee

/// Decoding / encoding coverage for M9 agent-lifecycle models, matching the live
/// `/api/servers` (create / recover / regenerate) and `/api/agent/latest-version`
/// payloads verified against the demo backend.
final class EnrollmentModelsDecodingTests: XCTestCase {
    private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
        try JSONDecoder.snakeCase.decode(T.self, from: Data(json.utf8))
    }

    private func encode<T: Encodable>(_ value: T) throws -> String {
        String(data: try JSONEncoder.snakeCase.encode(value), encoding: .utf8)!
    }

    // MARK: - Create

    func test_createServerResponse_decodes() throws {
        let json = """
        { "server_id": "8455ce93-41ba-4c9c-8969-19037f7ba711",
          "enrollment": {
            "id": "76623365-7cae-4d1c-932d-198a91d7da8c",
            "code": "SBENROLL-9F2A-7C41-DE08",
            "code_prefix": "gs1QPJIl",
            "expires_at": "2026-06-14T16:11:50.951993228+00:00"
          } }
        """
        let resp = try decode(CreateServerResponse.self, json)
        XCTAssertEqual(resp.serverId, "8455ce93-41ba-4c9c-8969-19037f7ba711")
        XCTAssertEqual(resp.enrollment.code, "SBENROLL-9F2A-7C41-DE08")
        XCTAssertEqual(resp.enrollment.codePrefix, "gs1QPJIl")
        XCTAssertEqual(resp.enrollment.id, "76623365-7cae-4d1c-932d-198a91d7da8c")
    }

    func test_createServerRequest_encodesGroupIdSnakeCase() throws {
        let json = try encode(CreateServerRequest(name: "edge-01", groupId: "grp-1"))
        XCTAssertTrue(json.contains("\"name\":\"edge-01\""))
        XCTAssertTrue(json.contains("\"group_id\":\"grp-1\""))
    }

    func test_createServerRequest_omitsNilGroupId() throws {
        let json = try encode(CreateServerRequest(name: "edge-02", groupId: nil))
        XCTAssertTrue(json.contains("\"name\":\"edge-02\""))
        XCTAssertFalse(json.contains("group_id"))
    }

    // MARK: - Recover / Regenerate

    func test_enrollmentOnlyResponse_decodes() throws {
        let json = """
        { "enrollment": {
            "id": "5c1c96ac-37df-45e3-9076-ebebe5511fc4",
            "code": "SBENROLL-AAAA-BBBB-CCCC",
            "code_prefix": "KhAzBgBs",
            "expires_at": "2026-06-15T18:00:00Z"
          } }
        """
        let resp = try decode(EnrollmentOnlyResponse.self, json)
        XCTAssertEqual(resp.enrollment.code, "SBENROLL-AAAA-BBBB-CCCC")
        XCTAssertEqual(resp.enrollment.codePrefix, "KhAzBgBs")
    }

    func test_recoverRequest_encodesRevokeFlag() throws {
        XCTAssertTrue(try encode(RecoverRequest(revokeImmediately: true)).contains("\"revoke_immediately\":true"))
        XCTAssertTrue(try encode(RecoverRequest(revokeImmediately: false)).contains("\"revoke_immediately\":false"))
    }

    func test_regenerateRequest_omitsNilExpectedId() throws {
        let json = try encode(RegenerateCodeRequest(expectedEnrollmentId: nil))
        XCTAssertFalse(json.contains("expected_enrollment_id"))
    }

    func test_regenerateRequest_encodesExpectedIdSnakeCase() throws {
        let json = try encode(RegenerateCodeRequest(expectedEnrollmentId: "enr-9"))
        XCTAssertTrue(json.contains("\"expected_enrollment_id\":\"enr-9\""))
    }

    // MARK: - Upgrade

    func test_upgradeRequest_encodes() throws {
        XCTAssertTrue(try encode(UpgradeRequest(version: "1.0.0-alpha.6")).contains("\"version\":\"1.0.0-alpha.6\""))
    }

    func test_latestAgentVersion_decodes() throws {
        let json = """
        { "version": "1.0.0-alpha.6", "released_at": "2026-05-31T11:19:01Z", "error": null }
        """
        let resp = try decode(LatestAgentVersion.self, json)
        XCTAssertEqual(resp.version, "1.0.0-alpha.6")
        XCTAssertEqual(resp.releasedAt, "2026-05-31T11:19:01Z")
        XCTAssertNil(resp.error)
    }

    func test_latestAgentVersion_decodesErrorOnly() throws {
        let json = """
        { "version": null, "released_at": null, "error": "release source unreachable" }
        """
        let resp = try decode(LatestAgentVersion.self, json)
        XCTAssertNil(resp.version)
        XCTAssertEqual(resp.error, "release source unreachable")
    }

    // MARK: - Install command

    @MainActor
    func test_installCommand_includesOriginAndCode() {
        let cmd = AgentLifecycleViewModel.installCommand(code: "SBENROLL-XYZ", serverUrl: "https://demo.serverbee.app")
        XCTAssertTrue(cmd.contains("--enrollment-code 'SBENROLL-XYZ'"))
        XCTAssertTrue(cmd.contains("--server-url 'https://demo.serverbee.app'"))
        XCTAssertTrue(cmd.contains("install.sh"))
    }

    @MainActor
    func test_installCommand_trimsWhitespaceOrigin() {
        let cmd = AgentLifecycleViewModel.installCommand(code: "C1", serverUrl: "  https://x.test  ")
        XCTAssertTrue(cmd.contains("--server-url 'https://x.test'"))
    }
}
