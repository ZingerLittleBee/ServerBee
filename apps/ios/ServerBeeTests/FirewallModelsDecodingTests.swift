import XCTest
@testable import ServerBee

/// Decoding + mapping coverage for M6 firewall blocklist models, matching the
/// server's `BlockListItem` / `ListResp` / `StatsResp` DTOs and the
/// `CreateBlockReq` request body.
final class FirewallModelsDecodingTests: XCTestCase {
    private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
        try JSONDecoder.snakeCase.decode(T.self, from: Data(json.utf8))
    }

    // MARK: - List response

    func test_blockListResponse_decodesItemsAndCursor() throws {
        let json = """
        { "items": [
            { "id": "b1", "target": "203.0.113.45/32", "family": 4,
              "cover_type": "all", "server_ids": null, "comment": "noisy scanner",
              "origin": "manual", "origin_event_id": null, "origin_rule_id": null,
              "created_by": "u1", "created_at": "2026-06-01T10:00:00Z" }
          ],
          "next_cursor": "2026-06-01T10:00:00Z" }
        """
        let resp = try decode(BlockListResponse.self, json)
        XCTAssertEqual(resp.items.count, 1)
        XCTAssertEqual(resp.nextCursor, "2026-06-01T10:00:00Z")
        let item = resp.items[0]
        XCTAssertEqual(item.id, "b1")
        XCTAssertEqual(item.target, "203.0.113.45/32")
        XCTAssertEqual(item.family, 4)
        XCTAssertFalse(item.isAuto)
        XCTAssertEqual(item.coverLabel, "All servers")
    }

    func test_blockListResponse_emptyWithNullCursor() throws {
        let json = #"{ "items": [], "next_cursor": null }"#
        let resp = try decode(BlockListResponse.self, json)
        XCTAssertTrue(resp.items.isEmpty)
        XCTAssertNil(resp.nextCursor)
    }

    // MARK: - Item mapping

    func test_autoBlock_isAutoTrue() throws {
        let json = """
        { "id": "b2", "target": "2001:db8::/64", "family": 6,
          "cover_type": "all", "server_ids": null, "comment": null,
          "origin": "auto", "origin_event_id": "evt-9", "origin_rule_id": "rule-3",
          "created_by": null, "created_at": "2026-06-02T08:00:00Z" }
        """
        let item = try decode(BlockListItem.self, json)
        XCTAssertTrue(item.isAuto)
        XCTAssertEqual(item.originEventId, "evt-9")
        XCTAssertEqual(item.family, 6)
    }

    func test_includeCover_labelCountsServers() throws {
        let json = """
        { "id": "b3", "target": "198.51.100.7/32", "family": 4,
          "cover_type": "include", "server_ids": ["s1", "s2", "s3"], "comment": null,
          "origin": "manual", "origin_event_id": null, "origin_rule_id": null,
          "created_by": "u1", "created_at": "2026-06-03T08:00:00Z" }
        """
        let item = try decode(BlockListItem.self, json)
        XCTAssertEqual(item.serverIds?.count, 3)
        XCTAssertEqual(item.coverLabel, "3 servers")
    }

    func test_excludeCover_labelDescribesException() throws {
        let json = """
        { "id": "b4", "target": "198.51.100.8/32", "family": 4,
          "cover_type": "exclude", "server_ids": ["s1"], "comment": null,
          "origin": "manual", "origin_event_id": null, "origin_rule_id": null,
          "created_by": "u1", "created_at": "2026-06-04T08:00:00Z" }
        """
        let item = try decode(BlockListItem.self, json)
        XCTAssertEqual(item.coverLabel, "All except 1")
    }

    // MARK: - Stats

    func test_stats_decodes() throws {
        let json = #"{ "total": 12, "auto": 7, "manual": 5, "v4": 9, "v6": 3 }"#
        let stats = try decode(FirewallStats.self, json)
        XCTAssertEqual(stats.total, 12)
        XCTAssertEqual(stats.auto, 7)
        XCTAssertEqual(stats.manual, 5)
        XCTAssertEqual(stats.v4, 9)
        XCTAssertEqual(stats.v6, 3)
    }

    // MARK: - Request encoding

    func test_createRequest_encodesSnakeCaseViaCodingKeys() throws {
        let req = CreateBlockRequest(
            target: "203.0.113.10",
            coverType: "include",
            serverIds: ["s1", "s2"],
            comment: "manual block"
        )
        let data = try JSONEncoder.snakeCase.encode(req)
        let json = String(data: data, encoding: .utf8)!
        XCTAssertTrue(json.contains("\"cover_type\":\"include\""))
        XCTAssertTrue(json.contains("\"server_ids\""))
        XCTAssertFalse(json.contains("coverType"))
        XCTAssertFalse(json.contains("serverIds"))
    }

    func test_createRequest_omitsNilOptionalFields() throws {
        let req = CreateBlockRequest(target: "203.0.113.11", coverType: "all", serverIds: nil, comment: nil)
        let data = try JSONEncoder.snakeCase.encode(req)
        let json = String(data: data, encoding: .utf8)!
        XCTAssertTrue(json.contains("\"target\":\"203.0.113.11\""))
        XCTAssertFalse(json.contains("server_ids"))
        XCTAssertFalse(json.contains("comment"))
    }
}
