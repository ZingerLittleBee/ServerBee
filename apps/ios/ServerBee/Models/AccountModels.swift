import Foundation

// MARK: - Password

/// Request body for `PUT /api/auth/password`.
struct ChangePasswordRequest: Encodable, Sendable {
    let oldPassword: String
    let newPassword: String

    enum CodingKeys: String, CodingKey {
        case oldPassword = "old_password"
        case newPassword = "new_password"
    }
}

// MARK: - Two-factor (TOTP)

/// `GET /api/auth/2fa/status`.
struct TwoFactorStatus: Decodable, Sendable {
    let enabled: Bool
}

/// `POST /api/auth/2fa/setup` — one-time secret, valid ~10 min.
struct TwoFactorSetup: Decodable, Sendable {
    let secret: String
    let otpauthUrl: String
    let qrCodeBase64: String

    enum CodingKeys: String, CodingKey {
        case secret
        case otpauthUrl = "otpauth_url"
        case qrCodeBase64 = "qr_code_base64"
    }
}

struct TwoFactorEnableRequest: Encodable, Sendable {
    let code: String
}

struct TwoFactorDisableRequest: Encodable, Sendable {
    let password: String
}

// MARK: - API keys

/// One API key (`GET/POST /api/auth/api-keys`). `key` (the plaintext
/// `serverbee_…` value) is populated ONLY in the create response and must be
/// shown to the user immediately — it is never returned again.
struct ApiKey: Decodable, Identifiable, Sendable {
    let id: String
    let name: String
    let keyPrefix: String
    let createdAt: String
    var key: String?

    enum CodingKeys: String, CodingKey {
        case id, name, key
        case keyPrefix = "key_prefix"
        case createdAt = "created_at"
    }
}

struct CreateApiKeyRequest: Encodable, Sendable {
    let name: String
}

// MARK: - Mobile devices

/// One signed-in mobile device (`GET /api/mobile/auth/devices`).
struct MobileDevice: Decodable, Identifiable, Sendable {
    let id: String
    let deviceName: String
    let installationId: String
    let createdAt: String
    let lastUsedAt: String

    enum CodingKeys: String, CodingKey {
        case id
        case deviceName = "device_name"
        case installationId = "installation_id"
        case createdAt = "created_at"
        case lastUsedAt = "last_used_at"
    }
}

// MARK: - About

/// `GET /api/about` — server-reported version.
struct AboutInfo: Decodable, Sendable {
    let version: String
}

/// Database (GeoIP / ASN) status (`GET /api/geoip/status`, `/api/asn/status`).
struct DbStatus: Decodable, Sendable {
    let installed: Bool
    var source: String?          // "custom" | "downloaded"
    var fileSize: Int64?
    var updatedAt: String?

    enum CodingKeys: String, CodingKey {
        case installed, source
        case fileSize = "file_size"
        case updatedAt = "updated_at"
    }
}

/// Result of a database download trigger (`POST /api/{geoip,asn}/download`).
struct DbDownloadResult: Decodable, Sendable {
    let success: Bool
    let message: String
}
