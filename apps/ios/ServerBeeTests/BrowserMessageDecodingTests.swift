import XCTest
@testable import ServerBee

final class BrowserMessageDecodingTests: XCTestCase {

    private func decode(_ json: String) throws -> BrowserMessage {
        let data = Data(json.utf8)
        return try JSONDecoder.snakeCase.decode(BrowserMessage.self, from: data)
    }

    func test_decode_fullSync() throws {
        let json = """
        {
          "type": "full_sync",
          "servers": [
            { "id": "s1", "name": "server-1", "online": true, "cpu_usage": 12.5 },
            { "id": "s2", "name": "server-2", "online": false }
          ]
        }
        """
        let msg = try decode(json)
        if case .fullSync(let servers) = msg {
            XCTAssertEqual(servers.count, 2)
            XCTAssertEqual(servers[0].id, "s1")
            XCTAssertEqual(servers[0].cpuUsage, 12.5)
            XCTAssertEqual(servers[1].online, false)
        } else {
            XCTFail("Expected .fullSync, got \(msg)")
        }
    }

    func test_decode_update() throws {
        let json = """
        {
          "type": "update",
          "servers": [
            { "id": "s1", "name": "server-1", "cpu_usage": 73.2 }
          ]
        }
        """
        let msg = try decode(json)
        if case .update(let servers) = msg {
            XCTAssertEqual(servers.count, 1)
            XCTAssertEqual(servers[0].cpuUsage, 73.2)
            // online is optional and was omitted: must remain nil.
            XCTAssertNil(servers[0].online)
        } else {
            XCTFail("Expected .update, got \(msg)")
        }
    }

    func test_decode_serverOnline() throws {
        let json = #"{"type":"server_online","server_id":"abc-123"}"#
        let msg = try decode(json)
        if case .serverOnline(let id) = msg {
            XCTAssertEqual(id, "abc-123")
        } else {
            XCTFail("Expected .serverOnline, got \(msg)")
        }
    }

    func test_decode_serverOffline() throws {
        let json = #"{"type":"server_offline","server_id":"abc-123"}"#
        let msg = try decode(json)
        if case .serverOffline(let id) = msg {
            XCTAssertEqual(id, "abc-123")
        } else {
            XCTFail("Expected .serverOffline, got \(msg)")
        }
    }

    func test_decode_capabilitiesChanged() throws {
        let json = """
        {
          "type": "capabilities_changed",
          "server_id": "abc",
          "capabilities": 56
        }
        """
        let msg = try decode(json)
        if case .capabilitiesChanged(let serverId, let caps) = msg {
            XCTAssertEqual(serverId, "abc")
            XCTAssertEqual(caps, 56)
        } else {
            XCTFail("Expected .capabilitiesChanged, got \(msg)")
        }
    }

    func test_decode_agentInfoUpdated() throws {
        let json = """
        {
          "type": "agent_info_updated",
          "server_id": "abc",
          "protocol_version": 3
        }
        """
        let msg = try decode(json)
        if case .agentInfoUpdated(let serverId, let version) = msg {
            XCTAssertEqual(serverId, "abc")
            XCTAssertEqual(version, 3)
        } else {
            XCTFail("Expected .agentInfoUpdated, got \(msg)")
        }
    }

    func test_decode_alertEvent_firing() throws {
        let json = """
        {
          "type": "alert_event",
          "alert_key": "rule-1:server-2",
          "status": "firing"
        }
        """
        let msg = try decode(json)
        if case .alertEvent(let key, let status) = msg {
            XCTAssertEqual(key, "rule-1:server-2")
            XCTAssertEqual(status, .firing)
        } else {
            XCTFail("Expected .alertEvent, got \(msg)")
        }
    }

    func test_decode_alertEvent_resolved() throws {
        let json = """
        {
          "type": "alert_event",
          "alert_key": "rule-1:server-2",
          "status": "resolved"
        }
        """
        let msg = try decode(json)
        if case .alertEvent(_, let status) = msg {
            XCTAssertEqual(status, .resolved)
        } else {
            XCTFail("Expected .alertEvent, got \(msg)")
        }
    }

    func test_decode_unknownType_throws() {
        let json = #"{"type":"definitely_not_a_real_case","server_id":"x"}"#
        XCTAssertThrowsError(try decode(json))
    }

    func test_decode_missingType_throws() {
        let json = #"{"server_id":"x"}"#
        XCTAssertThrowsError(try decode(json))
    }
}
