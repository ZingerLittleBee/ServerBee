import XCTest
@testable import ServerBee

/// Decoding / encoding coverage for M9 agent-lifecycle models, matching the live
/// `/api/servers`, Agent Authority, and `/api/agent/latest-version`
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
          "replayed": false,
          "outstanding_offer": null,
          "enrollment": {
            "id": "76623365-7cae-4d1c-932d-198a91d7da8c",
            "code": "SBENROLL-9F2A-7C41-DE08",
            "code_prefix": "gs1QPJIl",
            "expires_at": "2026-06-14T16:11:50.951993228+00:00"
          } }
        """
        let resp = try decode(CreateServerResponse.self, json)
        XCTAssertEqual(resp.serverId, "8455ce93-41ba-4c9c-8969-19037f7ba711")
        XCTAssertFalse(resp.replayed)
        XCTAssertEqual(resp.enrollment?.code, "SBENROLL-9F2A-7C41-DE08")
        XCTAssertEqual(resp.enrollment?.codePrefix, "gs1QPJIl")
        XCTAssertEqual(resp.enrollment?.id, "76623365-7cae-4d1c-932d-198a91d7da8c")
    }

    func test_createServerRequest_encodesGroupIdSnakeCase() throws {
        let json = try encode(CreateServerRequest(onboardingRequestId: "request-1", name: "edge-01", groupId: "grp-1"))
        XCTAssertTrue(json.contains("\"onboarding_request_id\":\"request-1\""))
        XCTAssertTrue(json.contains("\"name\":\"edge-01\""))
        XCTAssertTrue(json.contains("\"group_id\":\"grp-1\""))
    }

    func test_createServerRequest_omitsNilGroupId() throws {
        let json = try encode(CreateServerRequest(onboardingRequestId: "request-2", name: "edge-02", groupId: nil))
        XCTAssertTrue(json.contains("\"name\":\"edge-02\""))
        XCTAssertFalse(json.contains("group_id"))
    }

    // MARK: - Agent Authority

    func test_enrollmentOnlyResponse_decodes() throws {
        let json = """
        { "enrollment": {
            "id": "5c1c96ac-37df-45e3-9076-ebebe5511fc4",
            "code": "SBENROLL-AAAA-BBBB-CCCC",
            "code_prefix": "KhAzBgBs",
            "expires_at": "2026-06-15T18:00:00Z"
          } }
        """
        let resp = try decode(EnrollmentOfferResponse.self, json)
        XCTAssertEqual(resp.enrollment.code, "SBENROLL-AAAA-BBBB-CCCC")
        XCTAssertEqual(resp.enrollment.codePrefix, "KhAzBgBs")
    }

    func test_reenrollmentRequest_encodesGracefulMode() throws {
        XCTAssertTrue(try encode(ReenrollmentRequest(mode: .graceful)).contains("\"mode\":\"graceful\""))
    }

    func test_reenrollmentRequest_encodesEmergencyMode() throws {
        XCTAssertTrue(try encode(ReenrollmentRequest(mode: .emergency)).contains("\"mode\":\"emergency\""))
    }

    func test_createServerReplayDecodesOutstandingOfferWithoutPlaintext() throws {
        let json = """
        {
          "server_id": "srv-1",
          "replayed": true,
          "enrollment": null,
          "outstanding_offer": {
            "id": "offer-1",
            "code_prefix": "abcdef",
            "expires_at": "2026-07-13T00:10:00Z",
            "created_at": "2026-07-13T00:00:00Z"
          }
        }
        """
        let resp = try decode(CreateServerResponse.self, json)
        XCTAssertTrue(resp.replayed)
        XCTAssertNil(resp.enrollment)
        XCTAssertEqual(resp.outstandingOffer?.id, "offer-1")
    }

    func test_serverConfigUsesCanonicalAgentAuthorityProjection() throws {
        let json = """
        {
          "id": "srv-1",
          "name": "edge-1",
          "has_token": true,
          "agent_authority": {
            "status": "unclaimed",
            "outstanding_offer": {
              "id": "offer-1",
              "code_prefix": "abcdef",
              "expires_at": "2026-07-13T00:10:00Z",
              "created_at": "2026-07-13T00:00:00Z"
            }
          }
        }
        """
        let config = try decode(ServerConfig.self, json)
        XCTAssertFalse(config.isEnrolled)
        XCTAssertEqual(config.outstandingOffer?.id, "offer-1")
    }

    func test_outstandingOfferExpiryIsTerminalAtDeadline() throws {
        let offer = try decode(
            OutstandingOffer.self,
            #"{"id":"offer-1","expires_at":"2026-07-13T00:10:00Z"}"#
        )
        XCTAssertFalse(offer.isExpired(at: Date(timeIntervalSince1970: 1_783_901_399)))
        XCTAssertTrue(offer.isExpired(at: Date(timeIntervalSince1970: 1_783_901_400)))
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

    @MainActor
    func test_onboardingRequestIdStaysStableUntilReset() {
        let viewModel = AgentLifecycleViewModel()
        let first = viewModel.onboardingRequestId

        XCTAssertEqual(viewModel.onboardingRequestId, first)
        viewModel.resetOnboarding()

        XCTAssertNotEqual(viewModel.onboardingRequestId, first)
        XCTAssertNil(viewModel.issued)
        XCTAssertNil(viewModel.onboardingReplay)
    }
}
