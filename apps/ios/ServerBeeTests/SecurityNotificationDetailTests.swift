import XCTest
@testable import ServerBee

@MainActor
final class SecurityNotificationDetailTests: XCTestCase {
    func testCurrentUserWireContract() throws {
        let response = try JSONDecoder().decode(CurrentUserResponse.self, from:
            Data(#"{"user_id":"alice","username":"alice","role":"admin","must_change_password":false}"#.utf8))
        XCTAssertEqual(response.userId, "alice")
        XCTAssertEqual(response.role, "admin")
        XCTAssertFalse(response.mustChangePassword)
    }

    private func server() throws -> ServerConfig {
        try JSONDecoder().decode(ServerConfig.self, from: Data(#"{"id":"server","name":"Target"}"#.utf8))
    }

    private func event(serverId: String = "server") throws -> SecurityEventDto {
        let object: [String: Any] = ["id": "event", "server_id": serverId, "event_type": "port_scan", "severity": "high",
                                   "source_ip": "203.0.113.9", "started_at": "2026-10-03T00:00:00Z", "ended_at": "2026-10-03T00:00:30Z",
                                   "first_seen": false, "detector_source": "conntrack", "created_at": "2026-10-03T00:00:30Z"]
        return try JSONDecoder().decode(SecurityEventDto.self, from: JSONSerialization.data(withJSONObject: object))
    }

    func testExactSecurityTarget() async throws {
        let model = SecurityNotificationDetailModel()
        let target = try server()
        let detail = try event()
        let admin = CurrentUserResponse(userId: "alice", username: "alice", role: "admin", mustChangePassword: false)
        await model.load(target: SecurityNotificationTarget(serverId: "server", eventId: "event", userId: "alice"), loadUser: { admin },
                         loadServer: { target }, loadEvent: { detail }, isCurrent: { true })
        XCTAssertEqual(model.server?.id, "server")
        XCTAssertEqual(model.event?.id, "event")
        XCTAssertFalse(model.isLoading)
    }

    func testMemberCannotReadSecurityTarget() async throws {
        let model = SecurityNotificationDetailModel()
        var resourceReads = 0
        let target = try server()
        let detail = try event()
        await model.load(target: SecurityNotificationTarget(serverId: "server", eventId: "event", userId: "alice"),
                         loadUser: { CurrentUserResponse(userId: "alice", username: "alice", role: "member", mustChangePassword: false) },
                         loadServer: { resourceReads += 1; return target }, loadEvent: { resourceReads += 1; return detail }, isCurrent: { true })
        XCTAssertEqual(resourceReads, 0)
        XCTAssertNil(model.server)
        XCTAssertNil(model.event)
    }

    func testUnavailableSecurityTargets() async throws {
        let target = try server()
        let detail = try event()
        let mismatch = try event(serverId: "other-server")
        let admin = CurrentUserResponse(userId: "alice", username: "alice", role: "admin", mustChangePassword: false)
        for scenario in ["missing", "server-mismatch", "login-replaced", "role-downgraded", "account-mismatch"] {
            let model = SecurityNotificationDetailModel()
            var reads = 0
            await model.load(target: SecurityNotificationTarget(serverId: "server", eventId: "event", userId: "alice"), loadUser: {
                reads += 1
                if scenario == "account-mismatch" { return CurrentUserResponse(userId: "bob", username: "bob", role: "admin", mustChangePassword: false) }
                if scenario == "role-downgraded", reads > 1 { return CurrentUserResponse(userId: "alice", username: "alice", role: "member", mustChangePassword: false) }
                return admin
            }, loadServer: { target }, loadEvent: {
                if scenario == "missing" { throw APIError.httpError(statusCode: 404, data: Data()) }
                return scenario == "server-mismatch" ? mismatch : detail
            }, isCurrent: { scenario != "login-replaced" })
            XCTAssertNil(model.server, scenario)
            XCTAssertNil(model.event, scenario)
            XCTAssertFalse(model.isLoading)
        }
    }
}
