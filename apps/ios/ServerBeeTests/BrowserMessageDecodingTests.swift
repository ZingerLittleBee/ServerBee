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

    func test_decode_update_acceptsServerRuntimeMetricPayload() throws {
        let json = """
        {
          "type": "update",
          "servers": [
            {
              "id": "s1",
              "name": "server-1",
              "online": true,
              "cpu": 73.2,
              "mem_used": 4294967296,
              "mem_total": 8589934592,
              "disk_used": 10737418240,
              "disk_total": 21474836480,
              "net_in_speed": 12345,
              "net_out_speed": 67890,
              "load1": 1.25,
              "tcp_conn": 34,
              "udp_conn": 5,
              "process_count": 128,
              "country_code": "US"
            }
          ]
        }
        """
        let msg = try decode(json)
        guard case .update(let servers) = msg else {
            return XCTFail("Expected .update, got \(msg)")
        }

        XCTAssertEqual(servers[0].cpuUsage, 73.2)
        XCTAssertEqual(servers[0].memoryUsed, 4_294_967_296)
        XCTAssertEqual(servers[0].memoryTotal, 8_589_934_592)
        XCTAssertEqual(servers[0].diskUsed, 10_737_418_240)
        XCTAssertEqual(servers[0].diskTotal, 21_474_836_480)
        XCTAssertEqual(servers[0].networkIn, 12_345)
        XCTAssertEqual(servers[0].networkOut, 67_890)
        XCTAssertEqual(servers[0].load1, 1.25)
        XCTAssertEqual(servers[0].tcpCount, 34)
        XCTAssertEqual(servers[0].udpCount, 5)
        XCTAssertEqual(servers[0].processCount, 128)
        XCTAssertEqual(servers[0].country, "US")
    }

    /// Regression: the live browser WS frame sends `last_active` as a Unix epoch
    /// **integer** (and includes swap/transfer/disk-io/tags/has_token). The old
    /// decoder typed `last_active` as String, which threw `typeMismatch` and
    /// silently dropped EVERY full_sync/update frame — live metrics never showed.
    func test_decode_fullSync_acceptsRealBrowserWebSocketPayload() throws {
        let json = """
        {
          "type": "full_sync",
          "servers": [
            {
              "id": "s1",
              "name": "BWG",
              "online": true,
              "last_active": 1779834851,
              "uptime": 123456,
              "cpu": 3.34,
              "cpu_cores": 2,
              "mem_used": 1000,
              "mem_total": 2000,
              "swap_used": 0,
              "swap_total": 0,
              "disk_used": 5000,
              "disk_total": 10000,
              "net_in_speed": 12,
              "net_out_speed": 34,
              "net_in_transfer": 999,
              "net_out_transfer": 888,
              "load1": 0.1, "load5": 0.2, "load15": 0.3,
              "tcp_conn": 7, "udp_conn": 3, "process_count": 90,
              "disk_read_bytes_per_sec": 111,
              "disk_write_bytes_per_sec": 222,
              "tags": ["edge", "jp"],
              "has_token": true,
              "country_code": "JP"
            }
          ],
          "upgrades": []
        }
        """
        let msg = try decode(json)
        guard case .fullSync(let servers) = msg else {
            return XCTFail("Expected .fullSync, got \(msg)")
        }
        XCTAssertEqual(servers.count, 1)
        let s = servers[0]
        XCTAssertEqual(s.online, true)
        XCTAssertEqual(s.cpuUsage, 3.34)
        XCTAssertEqual(s.cpuCores, 2)
        XCTAssertEqual(s.netInTransfer, 999)
        XCTAssertEqual(s.diskReadPerSec, 111)
        XCTAssertEqual(s.tags, ["edge", "jp"])
        XCTAssertEqual(s.hasToken, true)
        XCTAssertEqual(s.country, "JP")
        // last_active integer is normalised to a parseable ISO string.
        XCTAssertNotNil(s.lastActiveAt)
        XCTAssertNotNil(s.lastActiveDate)
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
          "capabilities": 56,
          "agent_local_capabilities": 2047,
          "effective_capabilities": 56
        }
        """
        let msg = try decode(json)
        if case let .capabilitiesChanged(serverId, caps, agentLocal, effective) = msg {
            XCTAssertEqual(serverId, "abc")
            XCTAssertEqual(caps, 56)
            XCTAssertEqual(agentLocal, 2047)
            XCTAssertEqual(effective, 56)
        } else {
            XCTFail("Expected .capabilitiesChanged, got \(msg)")
        }
    }

    func test_decode_capabilitiesChanged_withoutOptionalMasks() throws {
        let json = """
        {
          "type": "capabilities_changed",
          "server_id": "abc",
          "capabilities": 56
        }
        """
        let msg = try decode(json)
        if case let .capabilitiesChanged(serverId, caps, agentLocal, effective) = msg {
            XCTAssertEqual(serverId, "abc")
            XCTAssertEqual(caps, 56)
            XCTAssertNil(agentLocal)
            XCTAssertNil(effective)
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

    func test_decode_metricRecord_acceptsServerRecordPayload() throws {
        let json = """
        {
          "time": "2026-05-20T10:30:00Z",
          "cpu": 42.5,
          "mem_used": 4294967296,
          "disk_used": 10737418240,
          "net_in_speed": 12345,
          "net_out_speed": 67890
        }
        """

        let record = try JSONDecoder.snakeCase.decode(MetricRecord.self, from: Data(json.utf8))

        XCTAssertEqual(record.timestamp, "2026-05-20T10:30:00Z")
        XCTAssertEqual(record.cpuUsage, 42.5)
        XCTAssertEqual(record.memoryUsed, 4_294_967_296)
        XCTAssertEqual(record.diskUsed, 10_737_418_240)
        XCTAssertEqual(record.networkIn, 12_345)
        XCTAssertEqual(record.networkOut, 67_890)
    }
}
