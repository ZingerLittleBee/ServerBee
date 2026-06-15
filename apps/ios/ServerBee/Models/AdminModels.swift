import Foundation

// MARK: - Users (admin-only)

/// One account (`GET /api/users`). Admin-only family.
struct AdminUser: Decodable, Identifiable, Sendable {
    let id: String
    let username: String
    let role: String                // "admin" | "member"
    let has2fa: Bool
    let createdAt: String
    let updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, username, role
        case has2fa = "has_2fa"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    var isAdmin: Bool { role == "admin" }
}

struct CreateUserRequest: Encodable, Sendable {
    let username: String
    let password: String
    var role: String?
}

/// Body for `PUT /api/users/{id}` — both fields optional (role change and/or
/// password reset).
struct UpdateUserRequest: Encodable, Sendable {
    var role: String?
    var password: String?
}

// MARK: - Audit logs (admin-only)

struct AuditLogEntry: Decodable, Identifiable, Sendable {
    let id: Int64
    let userId: String
    let action: String
    var detail: String?
    let ip: String
    let createdAt: String

    enum CodingKeys: String, CodingKey {
        case id, action, detail, ip
        case userId = "user_id"
        case createdAt = "created_at"
    }
}

/// `GET /api/audit-logs` — offset/limit paginated.
struct AuditLogPage: Decodable, Sendable {
    let entries: [AuditLogEntry]
    let total: Int

    enum CodingKeys: String, CodingKey {
        case entries, total
    }
}

/// `GET /api/audit-logs/options` — filter dropdown sources.
struct AuditLogOptions: Decodable, Sendable {
    let actions: [String]
    let users: [AuditUserOption]
}

struct AuditUserOption: Decodable, Identifiable, Sendable {
    let id: String
    let label: String
}

// MARK: - Rate limits (admin-only)

struct RateLimitBucket: Decodable, Identifiable, Sendable {
    let scope: String               // "login" | "register" | "public"
    let ip: String
    let count: Int
    let max: Int
    let windowSeconds: Int
    let windowStart: String
    let secondsRemaining: Int
    let blocked: Bool

    enum CodingKeys: String, CodingKey {
        case scope, ip, count, max, blocked
        case windowSeconds = "window_seconds"
        case windowStart = "window_start"
        case secondsRemaining = "seconds_remaining"
    }

    /// Stable identity for a bucket (scope + ip is unique per window).
    var id: String { "\(scope)|\(ip)" }
}

/// `GET /api/admin/rate-limit`.
struct RateLimitStatus: Decodable, Sendable {
    let entries: [RateLimitBucket]
    let loginMax: Int
    let registerMax: Int
    let publicMax: Int
    let authWindowSeconds: Int
    let publicWindowSeconds: Int

    enum CodingKeys: String, CodingKey {
        case entries
        case loginMax = "login_max"
        case registerMax = "register_max"
        case publicMax = "public_max"
        case authWindowSeconds = "auth_window_seconds"
        case publicWindowSeconds = "public_window_seconds"
    }
}
