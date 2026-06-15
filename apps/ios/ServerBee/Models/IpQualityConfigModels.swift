import Foundation

// Admin global IP-quality configuration (`/api/ip-quality/*`). The per-server
// snapshot/unlock RESULTS display lives in `IpQualityModels.swift`; this file
// covers the fleet-wide settings + the enable/delete writes on the service
// catalog. Custom-service CREATION (URL + headers + JSON match rules) stays on
// the web dashboard — authoring rule JSON is a desktop task.

/// Global IP-quality settings (`GET/PUT /api/ip-quality/settings`).
/// `checkIntervalHours` is the gap between automatic checks (1…168 hours).
struct IpQualitySettingModel: Decodable, Sendable {
    let checkIntervalHours: Int

    enum CodingKeys: String, CodingKey {
        case checkIntervalHours = "check_interval_hours"
    }
}

/// Body for `PUT /api/ip-quality/settings`.
struct UpdateIpQualitySettingRequest: Encodable, Sendable {
    let checkIntervalHours: Int

    enum CodingKeys: String, CodingKey {
        case checkIntervalHours = "check_interval_hours"
    }
}

/// Body for `PUT /api/ip-quality/services/{id}`. Mobile only flips `enabled`
/// (valid for both built-in and custom); omitted fields are preserved.
struct UpdateUnlockServiceRequest: Encodable, Sendable {
    var enabled: Bool?
}
