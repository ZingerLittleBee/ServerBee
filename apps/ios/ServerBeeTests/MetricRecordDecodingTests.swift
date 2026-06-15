import XCTest
@testable import ServerBee

/// Covers MetricRecord decoding of the `/api/servers/{id}/records` shape, where
/// disk I/O arrives as a double-encoded `disk_io_json` string (not flat fields)
/// and temperature is a top-level optional.
final class MetricRecordDecodingTests: XCTestCase {
    private func decode(_ json: String) throws -> MetricRecord {
        try JSONDecoder.snakeCase.decode(MetricRecord.self, from: Data(json.utf8))
    }

    func test_recordsShape_decodesTimeCpuTemperature() throws {
        let json = """
        { "id": 1, "server_id": "s1", "time": "2026-06-14T10:00:00Z",
          "cpu": 12.5, "mem_used": 1024, "disk_used": 2048,
          "net_in_speed": 100, "net_out_speed": 200, "load1": 0.4,
          "temperature": 47.5, "disk_io_json": null }
        """
        let record = try decode(json)
        XCTAssertEqual(record.timestamp, "2026-06-14T10:00:00Z")
        XCTAssertEqual(record.cpuUsage, 12.5)
        XCTAssertEqual(record.networkIn, 100)
        XCTAssertEqual(record.temperature, 47.5)
        XCTAssertTrue(record.diskIoSamples.isEmpty)
        XCTAssertNil(record.diskReadMerged)
    }

    func test_diskIoJson_parsesAndMergesAcrossDevices() throws {
        let json = """
        { "time": "2026-06-14T10:00:00Z", "cpu": 1,
          "disk_io_json": "[{\\"name\\":\\"sda\\",\\"read_bytes_per_sec\\":1000,\\"write_bytes_per_sec\\":500},{\\"name\\":\\"sdb\\",\\"read_bytes_per_sec\\":200,\\"write_bytes_per_sec\\":300}]" }
        """
        let record = try decode(json)
        XCTAssertEqual(record.diskIoSamples.count, 2)
        XCTAssertEqual(record.diskReadMerged, 1200)
        XCTAssertEqual(record.diskWriteMerged, 800)
    }

    func test_emptyDiskIoArray_yieldsNilMerged() throws {
        let record = try decode("""
        { "time": "2026-06-14T10:00:00Z", "cpu": 1, "disk_io_json": "[]" }
        """)
        XCTAssertTrue(record.diskIoSamples.isEmpty)
        XCTAssertNil(record.diskReadMerged)
        XCTAssertNil(record.diskWriteMerged)
    }

    func test_missingTemperature_isNil() throws {
        let record = try decode("""
        { "time": "2026-06-14T10:00:00Z", "cpu": 1 }
        """)
        XCTAssertNil(record.temperature)
    }
}
