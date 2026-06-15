import XCTest
@testable import ServerBee

/// Decoding coverage for M7 admin models, matching the live `/api/users`,
/// `/api/audit-logs`, and `/api/admin/rate-limit` payloads.
final class AdminModelsDecodingTests: XCTestCase {
    private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
        try JSONDecoder.snakeCase.decode(T.self, from: Data(json.utf8))
    }

    // MARK: - Users

    func test_adminUser_decodes() throws {
        let json = """
        { "id": "u1", "username": "admin", "role": "admin", "has_2fa": false,
          "created_at": "2026-03-29T14:05:17.908184902Z",
          "updated_at": "2026-03-29T14:05:17.908184902Z" }
        """
        let u = try decode(AdminUser.self, json)
        XCTAssertEqual(u.username, "admin")
        XCTAssertTrue(u.isAdmin)
        XCTAssertFalse(u.has2fa)
    }

    func test_createUserRequest_encodes() throws {
        let data = try JSONEncoder.snakeCase.encode(CreateUserRequest(username: "bob", password: "secret123", role: "member"))
        let json = String(data: data, encoding: .utf8)!
        XCTAssertTrue(json.contains("\"username\":\"bob\""))
        XCTAssertTrue(json.contains("\"role\":\"member\""))
    }

    func test_updateUserRequest_omitsNilFields() throws {
        let roleOnly = try JSONEncoder.snakeCase.encode(UpdateUserRequest(role: "admin", password: nil))
        let json = String(data: roleOnly, encoding: .utf8)!
        XCTAssertTrue(json.contains("\"role\":\"admin\""))
        XCTAssertFalse(json.contains("password"))
    }

    // MARK: - Audit logs

    func test_auditLogPage_decodes() throws {
        let json = """
        { "entries": [
            { "id": 24349, "user_id": "u1", "action": "server_created",
              "detail": "server_id=abc prefix=xyz", "ip": "100.64.0.14",
              "created_at": "2026-06-14T16:01:50.981325984+00:00" },
            { "id": 24348, "user_id": "u1", "action": "login", "detail": null,
              "ip": "100.64.0.8", "created_at": "2026-06-14T16:01:45.934670825+00:00" }
          ], "total": 24 }
        """
        let page = try decode(AuditLogPage.self, json)
        XCTAssertEqual(page.total, 24)
        XCTAssertEqual(page.entries.count, 2)
        XCTAssertEqual(page.entries[0].action, "server_created")
        XCTAssertNil(page.entries[1].detail)
    }

    func test_auditLogOptions_decodes() throws {
        let json = """
        { "actions": ["login", "server_created"],
          "users": [{ "id": "u1", "label": "admin" }] }
        """
        let opts = try decode(AuditLogOptions.self, json)
        XCTAssertEqual(opts.actions, ["login", "server_created"])
        XCTAssertEqual(opts.users.first?.label, "admin")
    }

    // MARK: - Rate limits

    func test_rateLimitStatus_decodes() throws {
        let json = """
        { "entries": [
            { "scope": "login", "ip": "100.64.0.13", "count": 2, "max": 5,
              "window_seconds": 900, "window_start": "2026-06-14T16:05:27.858878918+00:00",
              "seconds_remaining": 265, "blocked": false }
          ],
          "login_max": 5, "register_max": 3, "public_max": 60,
          "auth_window_seconds": 900, "public_window_seconds": 60 }
        """
        let status = try decode(RateLimitStatus.self, json)
        XCTAssertEqual(status.loginMax, 5)
        XCTAssertEqual(status.publicWindowSeconds, 60)
        XCTAssertEqual(status.entries.count, 1)
        let bucket = status.entries[0]
        XCTAssertEqual(bucket.scope, "login")
        XCTAssertEqual(bucket.id, "login|100.64.0.13")
        XCTAssertEqual(bucket.secondsRemaining, 265)
        XCTAssertFalse(bucket.blocked)
    }
}
