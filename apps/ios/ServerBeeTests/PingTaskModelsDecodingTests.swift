import XCTest
@testable import ServerBee

final class PingTaskModelsDecodingTests: XCTestCase {

    func test_decode_pingTask_fromEntityModel() throws {
        let json = """
        {
          "id": "pt1",
          "name": "Cloudflare",
          "probe_type": "icmp",
          "target": "1.1.1.1",
          "interval": 60,
          "server_ids_json": "[\\"s1\\",\\"s2\\"]",
          "enabled": true,
          "created_at": "2026-06-15T10:00:00Z"
        }
        """
        let task = try JSONDecoder.snakeCase.decode(PingTask.self, from: Data(json.utf8))
        XCTAssertEqual(task.id, "pt1")
        XCTAssertEqual(task.name, "Cloudflare")
        XCTAssertEqual(task.probeType, .icmp)
        XCTAssertEqual(task.target, "1.1.1.1")
        XCTAssertEqual(task.interval, 60)
        XCTAssertEqual(task.enabled, true)
        // server_ids_json is a JSON STRING column → decoded by the computed prop.
        XCTAssertEqual(task.serverIds, ["s1", "s2"])
    }

    func test_decode_pingTask_emptyServerIdsMeansAll() throws {
        let json = #"{"id":"pt2","name":"All","probe_type":"http","target":"https://x.test","interval":120,"server_ids_json":"[]","enabled":false,"created_at":"2026-06-15T10:00:00Z"}"#
        let task = try JSONDecoder.snakeCase.decode(PingTask.self, from: Data(json.utf8))
        XCTAssertEqual(task.probeType, .http)
        XCTAssertTrue(task.serverIds.isEmpty)
        XCTAssertEqual(task.enabled, false)
    }

    func test_decode_probeType_allCases() throws {
        for raw in ["icmp", "tcp", "http"] {
            let json = "{\"id\":\"x\",\"name\":\"n\",\"probe_type\":\"\(raw)\",\"target\":\"t\",\"interval\":30,\"server_ids_json\":null,\"enabled\":true,\"created_at\":\"2026-06-15T10:00:00Z\"}"
            let task = try JSONDecoder.snakeCase.decode(PingTask.self, from: Data(json.utf8))
            XCTAssertEqual(task.probeType.rawValue, raw)
            // Null server_ids_json also means all servers.
            XCTAssertTrue(task.serverIds.isEmpty)
        }
    }

    func test_encode_createRequest_usesServerIdsArrayAndSnakeCase() throws {
        let request = CreatePingTaskRequest(
            name: "API", probeType: .tcp, target: "api.test:443",
            interval: 300, serverIds: ["s1"], enabled: true
        )
        let data = try JSONEncoder.snakeCase.encode(request)
        let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        XCTAssertEqual(obj?["probe_type"] as? String, "tcp")
        XCTAssertEqual(obj?["server_ids"] as? [String], ["s1"])
        XCTAssertEqual(obj?["interval"] as? Int, 300)
        XCTAssertNil(obj?["server_ids_json"], "request uses server_ids array, not the json-string column")
    }

    func test_encode_updateRequest_enabledOnlyOmitsOtherKeys() throws {
        let request = UpdatePingTaskRequest(enabled: false)
        let data = try JSONEncoder.snakeCase.encode(request)
        let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        XCTAssertEqual(obj?["enabled"] as? Bool, false)
        // Synthesized Encodable omits nil optionals → toggle PUT sends only enabled.
        XCTAssertNil(obj?["name"])
        XCTAssertNil(obj?["probe_type"])
        XCTAssertNil(obj?["target"])
        XCTAssertNil(obj?["interval"])
        XCTAssertNil(obj?["server_ids"])
    }
}
