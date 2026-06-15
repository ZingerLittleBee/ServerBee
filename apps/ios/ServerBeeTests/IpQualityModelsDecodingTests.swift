import XCTest
@testable import ServerBee

/// Decoding + mapping coverage for M6 IP-quality models, matching the live
/// `GET /api/ip-quality/servers/{id}` and `/api/ip-quality/services` payloads.
final class IpQualityModelsDecodingTests: XCTestCase {
    private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
        try JSONDecoder.snakeCase.decode(T.self, from: Data(json.utf8))
    }

    // MARK: - Per-server payload

    func test_serverIpQuality_decodesNestedSnapshotAndResults() throws {
        let json = """
        { "server_id": "s1",
          "unlock_results": [
            { "id": "r1", "server_id": "s1", "service_id": "svc-1", "status": "failed",
              "region": null, "latency_ms": 10004, "detail": null,
              "checked_at": "2026-05-26T22:34:21.503090697+00:00" },
            { "id": "r2", "server_id": "s1", "service_id": "svc-6", "status": "unlocked",
              "region": "JP", "latency_ms": 18, "detail": null,
              "checked_at": "2026-05-26T22:34:21.503090697+00:00" }
          ],
          "ip_quality": {
            "ip": "154.36.181.226", "asn": null, "as_org": null,
            "country": "JP", "region": null, "city": null,
            "ip_type": "unknown", "is_proxy": false, "is_vpn": false,
            "is_hosting": false, "risk_score": null, "risk_level": "unknown",
            "is_tor": false, "is_abuser": false, "is_mobile": false,
            "asn_abuser_score": null, "abuse_email": null,
            "checked_at": "2026-05-26T15:26:09.683946314Z" }
        }
        """
        let data = try decode(ServerIpQualityData.self, json)
        XCTAssertEqual(data.serverId, "s1")
        XCTAssertEqual(data.unlockResults.count, 2)
        XCTAssertEqual(data.unlockResults[0].status, "failed")
        XCTAssertEqual(data.unlockResults[0].latencyMs, 10004)
        XCTAssertEqual(data.unlockResults[1].region, "JP")
        XCTAssertEqual(data.ipQuality?.ip, "154.36.181.226")
        XCTAssertEqual(data.ipQuality?.country, "JP")
        XCTAssertEqual(data.ipQuality?.riskLevel, "unknown")
        XCTAssertNil(data.ipQuality?.riskScore)
        // No flags set on a clean residential-ish IP
        XCTAssertTrue(data.ipQuality?.flags.isEmpty ?? false)
        // location collapses to country when city/region absent
        XCTAssertEqual(data.ipQuality?.location, "JP")
    }

    func test_snapshot_flagsReflectActiveBits() throws {
        let json = """
        { "ip": "1.2.3.4", "asn": "AS13335", "as_org": "Cloudflare",
          "country": "US", "region": "CA", "city": "San Francisco",
          "ip_type": "datacenter", "is_proxy": true, "is_vpn": true,
          "is_hosting": true, "risk_score": 88, "risk_level": "high",
          "is_tor": false, "is_abuser": true, "is_mobile": false,
          "asn_abuser_score": 42, "abuse_email": "abuse@example.com",
          "checked_at": "2026-06-01T00:00:00Z" }
        """
        let snap = try decode(IpQualitySnapshot.self, json)
        XCTAssertEqual(snap.riskScore, 88)
        XCTAssertEqual(snap.riskLevel, "high")
        XCTAssertEqual(snap.asnAbuserScore, 42)
        XCTAssertEqual(snap.flags, ["Proxy", "VPN", "Hosting", "Abuser"])
        XCTAssertEqual(snap.location, "San Francisco, CA, US")
    }

    // MARK: - Services catalog (ignores extra builtin fields)

    func test_unlockService_decodesIgnoringExtraFields() throws {
        let json = """
        { "id": "01960000-0000-7000-8000-000000000006", "key": "chatgpt",
          "name": "ChatGPT", "category": "ai", "popularity": 100,
          "is_builtin": true, "enabled": true, "detector": "chatgpt",
          "request": null, "rules": null,
          "created_at": "2026-05-22T00:00:00Z", "updated_at": "2026-05-22T00:00:00Z" }
        """
        let svc = try decode(UnlockService.self, json)
        XCTAssertEqual(svc.key, "chatgpt")
        XCTAssertEqual(svc.name, "ChatGPT")
        XCTAssertEqual(svc.category, "ai")
        XCTAssertTrue(svc.enabled)
    }

    // MARK: - Presentation helpers

    func test_unlockStatusStyle_labels() {
        XCTAssertEqual(UnlockStatusStyle.label("unlocked"), "Unlocked")
        XCTAssertEqual(UnlockStatusStyle.label("failed"), "Failed")
        XCTAssertEqual(UnlockStatusStyle.label("unsupported"), "Unsupported")
        XCTAssertEqual(UnlockStatusStyle.label("weird"), "Weird")
    }

    func test_ipRisk_label() {
        XCTAssertEqual(IpRisk.label("high"), "High")
        XCTAssertEqual(IpRisk.label("low"), "Low")
    }
}
