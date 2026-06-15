import XCTest
@testable import ServerBee

/// Decoding / encoding coverage for M11 alert-config models, matching the live
/// `/api/notifications` and `/api/alert-rules` payloads (both admin-only).
final class NotificationModelsDecodingTests: XCTestCase {
    private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
        try JSONDecoder.snakeCase.decode(T.self, from: Data(json.utf8))
    }

    private func encode<T: Encodable>(_ value: T) throws -> String {
        String(data: try JSONEncoder.snakeCase.encode(value), encoding: .utf8)!
    }

    // MARK: - Notification channels

    func test_notificationChannel_decodes() throws {
        let json = """
        { "id": "c1", "name": "Ops Telegram", "notify_type": "telegram",
          "config_json": "{\\"chat_id\\":\\"123\\"}", "enabled": true,
          "created_at": "2026-06-14T16:11:50Z" }
        """
        let channel = try decode(NotificationChannel.self, json)
        XCTAssertEqual(channel.id, "c1")
        XCTAssertEqual(channel.name, "Ops Telegram")
        XCTAssertEqual(channel.notifyType, "telegram")
        XCTAssertTrue(channel.enabled)
        XCTAssertEqual(channel.typeLabel, "Telegram")
        XCTAssertEqual(channel.typeIcon, "paperplane")
    }

    func test_notificationChannel_unknownType_fallsBack() throws {
        let json = """
        { "id": "c9", "name": "Custom", "notify_type": "matrix",
          "config_json": "{}", "enabled": false, "created_at": "2026-06-14T16:11:50Z" }
        """
        let channel = try decode(NotificationChannel.self, json)
        XCTAssertEqual(channel.typeLabel, "Matrix")
        XCTAssertEqual(channel.typeIcon, "bell")
        XCTAssertFalse(channel.enabled)
    }

    func test_notificationChannel_arrayDecodes() throws {
        let json = """
        [ { "id": "c1", "name": "A", "notify_type": "webhook", "config_json": "{}",
            "enabled": true, "created_at": "2026-06-14T16:11:50Z" },
          { "id": "c2", "name": "B", "notify_type": "email", "config_json": "{}",
            "enabled": false, "created_at": "2026-06-14T16:11:50Z" } ]
        """
        let list = try decode([NotificationChannel].self, json)
        XCTAssertEqual(list.count, 2)
        XCTAssertEqual(list[0].typeIcon, "link")
        XCTAssertEqual(list[1].typeIcon, "envelope")
    }

    // MARK: - Alert rules

    func test_alertRule_decodes_ignoringExtraFields() throws {
        // Includes fields the mobile model deliberately drops (rules_json,
        // actions_json, fail/recover_trigger_tasks) to confirm they're ignored.
        let json = """
        { "id": "r1", "name": "High CPU", "enabled": true, "rules_json": "[]",
          "trigger_mode": "any", "notification_group_id": "g1",
          "fail_trigger_tasks": null, "recover_trigger_tasks": null,
          "cover_type": "all", "server_ids_json": null, "actions_json": null,
          "created_at": "2026-06-14T16:11:50Z", "updated_at": "2026-06-14T16:11:50Z" }
        """
        let rule = try decode(AlertRule.self, json)
        XCTAssertEqual(rule.id, "r1")
        XCTAssertEqual(rule.name, "High CPU")
        XCTAssertTrue(rule.enabled)
        XCTAssertEqual(rule.triggerMode, "any")
        XCTAssertEqual(rule.notificationGroupId, "g1")
        XCTAssertEqual(rule.coverType, "all")
        XCTAssertEqual(rule.coverLabel, "All servers")
        XCTAssertNil(rule.serverIdsJson)
    }

    func test_alertRule_coverLabel_variants() throws {
        func rule(cover: String) throws -> AlertRule {
            try decode(AlertRule.self, """
            { "id": "r", "name": "n", "enabled": true, "rules_json": "[]",
              "trigger_mode": "all", "notification_group_id": null,
              "cover_type": "\(cover)", "server_ids_json": "[\\"s1\\"]",
              "created_at": "2026-06-14T16:11:50Z", "updated_at": "2026-06-14T16:11:50Z" }
            """)
        }
        XCTAssertEqual(try rule(cover: "include").coverLabel, "Selected servers")
        XCTAssertEqual(try rule(cover: "exclude").coverLabel, "All except selected")
    }

    // MARK: - Toggle request

    func test_toggleEnabledRequest_encodesEnabledTrue() throws {
        let json = try encode(ToggleEnabledRequest(enabled: true))
        XCTAssertEqual(json, "{\"enabled\":true}")
    }

    func test_toggleEnabledRequest_encodesEnabledFalse() throws {
        let json = try encode(ToggleEnabledRequest(enabled: false))
        XCTAssertEqual(json, "{\"enabled\":false}")
    }
}
