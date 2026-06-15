import XCTest
@testable import ServerBee

/// Guards the maintenance write contract: `server_ids_json` is an ARRAY on the
/// wire (not a JSON string), keys are snake_case, and WireDate emits RFC3339.
final class MaintenanceRequestEncodingTests: XCTestCase {
    private func encodeToObject(_ value: some Encodable) throws -> [String: Any] {
        let data = try JSONEncoder.snakeCase.encode(value)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    func test_createMaintenance_encodesServerIdsAsArray() throws {
        let request = CreateMaintenanceRequest(
            title: "DB upgrade",
            description: "rolling",
            startAt: "2026-06-20T00:00:00Z",
            endAt: "2026-06-20T02:00:00Z",
            serverIdsJson: ["s1", "s2"],
            isPublic: true
        )
        let object = try encodeToObject(request)
        XCTAssertEqual(object["title"] as? String, "DB upgrade")
        XCTAssertEqual(object["start_at"] as? String, "2026-06-20T00:00:00Z")
        XCTAssertEqual(object["end_at"] as? String, "2026-06-20T02:00:00Z")
        XCTAssertEqual(object["is_public"] as? Bool, true)
        XCTAssertEqual(object["server_ids_json"] as? [String], ["s1", "s2"])
    }

    func test_createMaintenance_nilServerIds_appliesToAll() throws {
        let request = CreateMaintenanceRequest(
            title: "x", description: nil,
            startAt: "2026-06-20T00:00:00Z", endAt: "2026-06-20T01:00:00Z",
            serverIdsJson: nil, isPublic: false
        )
        let object = try encodeToObject(request)
        XCTAssertNil(object["server_ids_json"])
        XCTAssertNil(object["description"])
    }

    func test_wireDate_emitsRFC3339Zulu() {
        let date = Date(timeIntervalSince1970: 1_750_000_000)
        let string = WireDate.string(from: date)
        XCTAssertTrue(string.hasSuffix("Z"), "expected Zulu RFC3339, got \(string)")
        XCTAssertNotNil(ISO8601DateFormatter.shared.date(from: string), "round-trips through the shared parser")
    }
}
