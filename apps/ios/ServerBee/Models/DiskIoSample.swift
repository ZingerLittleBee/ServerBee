import Foundation

/// One device's disk I/O rate, parsed from a record's `disk_io_json` blob.
///
/// The `/api/servers/{id}/records` endpoint does NOT carry flat
/// `disk_read_bytes_per_sec` / `disk_write_bytes_per_sec` columns — per-device
/// disk I/O lives only inside the double-encoded `disk_io_json` string, a JSON
/// array of these objects (sorted by name; per-device read/write averaged over
/// the hour for the `hourly` interval).
struct DiskIoSample: Decodable, Sendable {
    let name: String
    let readBytesPerSec: Int64
    let writeBytesPerSec: Int64

    enum CodingKeys: String, CodingKey {
        case name
        case readBytesPerSec = "read_bytes_per_sec"
        case writeBytesPerSec = "write_bytes_per_sec"
    }
}
