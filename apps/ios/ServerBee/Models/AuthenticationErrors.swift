import Foundation

// MARK: - Auth Errors

enum AuthError: Error, LocalizedError {
    case noServerUrl
    case staleIdentity
    case refreshUnauthorized           // server returned 401 — credentials revoked
    case refreshNetworkFailure(Error?) // transient: no network, 5xx, timeout
    case invalidCredentials
    case twoFactorRequired
    case tooManyAttempts
    case networkError(Error)
    case secureLogoutNeedsConnection
    case cleanupCapacity
    case invalidSessionResponse

    var errorDescription: String? {
        switch self {
        case .invalidSessionResponse:
            return String(localized: "The server returned an invalid session identity. Please retry.")
        case .secureLogoutNeedsConnection:
            return String(localized: "Connect to your server to complete secure sign-out before signing in again.")
        case .cleanupCapacity:
            return String(localized: "Pending sign-outs need to finish before another login. Connect to your servers and retry.")
        case .noServerUrl:
            return String(localized: "No server URL configured")
        case .refreshUnauthorized, .staleIdentity:
            return String(localized: "Session expired. Please log in again.")
        case .refreshNetworkFailure:
            return String(localized: "Could not reach the server. Please check your connection.")
        case .invalidCredentials:
            return String(localized: "Invalid username or password")
        case .twoFactorRequired:
            return String(localized: "Two-factor authentication is required")
        case .tooManyAttempts:
            return String(localized: "Too many attempts. Please try again later.")
        case .networkError(let error):
            return String(localized: "Network error: \(error.localizedDescription)")
        }
    }
}

/// Immutable identity for an in-flight mobile registration or logout request.
struct MobileAuthenticationContext: Sendable {
    let serverUrl: String
    let userId: String
    let installationId: String
    let generation: UUID
    let accessToken: String
    let revocationToken: String?
    // The saved proof may belong to a session replaced by an older Server.
    // Capture this login's current secret before rotation can lose its response.
    // It is sent only to the deletion endpoint, never as an access credential.
    let refreshToken: String?
}
