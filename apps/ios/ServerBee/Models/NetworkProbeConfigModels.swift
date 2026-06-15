import Foundation

// Admin global network-probe configuration (`/api/network-probes/*`). The
// per-server probe RESULTS live in `NetworkProbeModels.swift`; this file covers
// the fleet-wide TARGET catalog + global SETTINGS that admins manage.

extension NetworkProbeTarget {
    /// Preset targets ship with the server and carry a `source` ("preset:{group}");
    /// custom targets (admin-created) have no source and can be edited/deleted.
    var isPreset: Bool { source != nil }

    /// Probe type as the shared enum (icmp/tcp/http), defaulting to icmp.
    var probeTypeEnum: PingProbeType { PingProbeType(rawValue: probeType) ?? .icmp }
}

/// Global probe settings (`GET/PUT /api/network-probes/setting`). `interval` is
/// in seconds (30…600); `packetCount` is packets per probe (5…20);
/// `defaultTargetIds` are assigned to newly enrolled servers.
struct NetworkProbeSetting: Decodable, Sendable {
    let interval: Int
    let packetCount: Int
    let defaultTargetIds: [String]

    enum CodingKeys: String, CodingKey {
        case interval
        case packetCount = "packet_count"
        case defaultTargetIds = "default_target_ids"
    }
}

/// Body for `PUT /api/network-probes/setting` — all fields required.
struct UpdateProbeSettingRequest: Encodable, Sendable {
    let interval: Int
    let packetCount: Int
    let defaultTargetIds: [String]

    enum CodingKeys: String, CodingKey {
        case interval
        case packetCount = "packet_count"
        case defaultTargetIds = "default_target_ids"
    }
}

/// Body for `POST /api/network-probes/targets` — all fields required.
struct CreateProbeTargetRequest: Encodable, Sendable {
    let name: String
    let provider: String
    let location: String
    let target: String
    let probeType: String

    enum CodingKeys: String, CodingKey {
        case name, provider, location, target
        case probeType = "probe_type"
    }
}

/// Body for `PUT /api/network-probes/targets/{id}` — omitted fields preserved.
struct UpdateProbeTargetRequest: Encodable, Sendable {
    var name: String?
    var provider: String?
    var location: String?
    var target: String?
    var probeType: String?

    enum CodingKeys: String, CodingKey {
        case name, provider, location, target
        case probeType = "probe_type"
    }
}
