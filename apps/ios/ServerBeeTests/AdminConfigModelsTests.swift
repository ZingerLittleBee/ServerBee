import XCTest
@testable import ServerBee

/// Decode/encode coverage for the P1 admin-config models: status page,
/// network-probe global config, and IP-quality config. Guards the snake_case
/// wire contract (the project coders apply no key strategy).
final class AdminConfigModelsTests: XCTestCase {

    // MARK: - Status Page

    func test_decode_statusPageConfig() throws {
        let json = """
        {
          "id": "default",
          "title": "My Status",
          "description": "hi",
          "server_ids_json": "[\\"s1\\",\\"s2\\"]",
          "group_by_server_group": false,
          "enabled": true,
          "uptime_yellow_threshold": 99.0,
          "uptime_red_threshold": 95.0,
          "show_ip_quality": true,
          "default_layout": "grid",
          "show_server_detail": true,
          "show_network": false,
          "show_incidents": true,
          "show_maintenance": false,
          "created_at": "2026-06-15T10:00:00Z",
          "updated_at": "2026-06-15T10:00:00Z"
        }
        """
        let cfg = try JSONDecoder.snakeCase.decode(StatusPageConfig.self, from: Data(json.utf8))
        XCTAssertEqual(cfg.title, "My Status")
        XCTAssertEqual(cfg.serverIds, ["s1", "s2"])
        XCTAssertEqual(cfg.defaultLayout, "grid")
        XCTAssertEqual(cfg.uptimeYellowThreshold, 99.0)
        XCTAssertTrue(cfg.showIpQuality)
        XCTAssertFalse(cfg.showNetwork)
    }

    func test_encode_statusPageRequest_blankDescriptionBecomesNull() throws {
        let request = UpdateStatusPageRequest(
            title: "T", description: "   ", serverIds: ["s1"], enabled: true,
            uptimeYellowThreshold: 99.5, uptimeRedThreshold: 90.0, showIpQuality: false,
            defaultLayout: "list", showServerDetail: true, showNetwork: true,
            showIncidents: false, showMaintenance: false
        )
        let data = try JSONEncoder.snakeCase.encode(request)
        let obj = try JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]) as? [String: Any]
        XCTAssertEqual(obj?["title"] as? String, "T")
        XCTAssertEqual(obj?["server_ids"] as? [String], ["s1"])
        XCTAssertEqual(obj?["default_layout"] as? String, "list")
        // Blank description encodes as explicit JSON null (clears the column).
        XCTAssertTrue(obj?["description"] is NSNull)
        XCTAssertEqual(obj?["uptime_yellow_threshold"] as? Double, 99.5)
        XCTAssertEqual(obj?["show_server_detail"] as? Bool, true)
    }

    func test_encode_statusPageRequest_keepsDescription() throws {
        let request = UpdateStatusPageRequest(
            title: "T", description: " hello ", serverIds: [], enabled: false,
            uptimeYellowThreshold: 99, uptimeRedThreshold: 95, showIpQuality: false,
            defaultLayout: "grid", showServerDetail: false, showNetwork: false,
            showIncidents: false, showMaintenance: false
        )
        let data = try JSONEncoder.snakeCase.encode(request)
        let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        XCTAssertEqual(obj?["description"] as? String, "hello")
    }

    // MARK: - Network Probe

    func test_decode_networkProbeSetting() throws {
        let json = """
        { "interval": 120, "packet_count": 15, "default_target_ids": ["a", "b"] }
        """
        let s = try JSONDecoder.snakeCase.decode(NetworkProbeSetting.self, from: Data(json.utf8))
        XCTAssertEqual(s.interval, 120)
        XCTAssertEqual(s.packetCount, 15)
        XCTAssertEqual(s.defaultTargetIds, ["a", "b"])
    }

    func test_networkProbeTarget_isPreset() throws {
        let preset = """
        { "id": "p1", "name": "CT", "provider": "ct", "location": "CN", "target": "1.1.1.1",
          "probe_type": "icmp", "source": "preset:cn" }
        """
        let custom = """
        { "id": "c1", "name": "Mine", "provider": "custom", "location": "X", "target": "9.9.9.9",
          "probe_type": "tcp" }
        """
        let p = try JSONDecoder.snakeCase.decode(NetworkProbeTarget.self, from: Data(preset.utf8))
        let c = try JSONDecoder.snakeCase.decode(NetworkProbeTarget.self, from: Data(custom.utf8))
        XCTAssertTrue(p.isPreset)
        XCTAssertFalse(c.isPreset)
        XCTAssertEqual(c.probeTypeEnum, .tcp)
    }

    func test_encode_createProbeTarget_snakeCase() throws {
        let request = CreateProbeTargetRequest(
            name: "N", provider: "cu", location: "L", target: "t", probeType: "http"
        )
        let obj = try JSONSerialization.jsonObject(
            with: JSONEncoder.snakeCase.encode(request)
        ) as? [String: Any]
        XCTAssertEqual(obj?["probe_type"] as? String, "http")
        XCTAssertEqual(obj?["provider"] as? String, "cu")
    }

    func test_encode_updateProbeTarget_omitsNil() throws {
        let obj = try JSONSerialization.jsonObject(
            with: JSONEncoder.snakeCase.encode(UpdateProbeTargetRequest(name: "only"))
        ) as? [String: Any]
        XCTAssertEqual(obj?["name"] as? String, "only")
        XCTAssertNil(obj?["target"])
        XCTAssertNil(obj?["probe_type"])
    }

    // MARK: - IP Quality

    func test_decode_ipQualitySetting() throws {
        let s = try JSONDecoder.snakeCase.decode(
            IpQualitySettingModel.self, from: Data("{ \"check_interval_hours\": 24 }".utf8)
        )
        XCTAssertEqual(s.checkIntervalHours, 24)
    }

    func test_decode_unlockService_builtinAndCategory() throws {
        let builtin = """
        { "id": "1", "key": "netflix", "name": "Netflix", "category": "streaming",
          "enabled": true, "popularity": 100, "is_builtin": true }
        """
        let custom = """
        { "id": "2", "key": "custom_ab12", "name": "Mine", "category": null,
          "enabled": false, "popularity": 0, "is_builtin": false }
        """
        let b = try JSONDecoder.snakeCase.decode(UnlockService.self, from: Data(builtin.utf8))
        let c = try JSONDecoder.snakeCase.decode(UnlockService.self, from: Data(custom.utf8))
        XCTAssertTrue(b.builtin)
        XCTAssertEqual(b.categoryLabel, "Streaming")
        XCTAssertFalse(c.builtin)
        XCTAssertEqual(c.categoryLabel, String(localized: "Other"))
    }

    func test_encode_updateUnlockService_enabledOnly() throws {
        let obj = try JSONSerialization.jsonObject(
            with: JSONEncoder.snakeCase.encode(UpdateUnlockServiceRequest(enabled: false))
        ) as? [String: Any]
        XCTAssertEqual(obj?["enabled"] as? Bool, false)
    }
}
