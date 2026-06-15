import SwiftUI

/// Backs the password-change and two-factor (TOTP) screens. All endpoints are
/// member-ok; the server re-verifies the current password / TOTP code.
@MainActor
@Observable
final class AccountSecurityViewModel {
    // 2FA status
    var twoFactorEnabled: Bool?
    var isLoadingStatus = false

    // Enrollment
    var setup: TwoFactorSetup?
    var isEnrolling = false
    var enrollError: String?

    // Generic action state
    var isWorking = false
    var actionError: String?

    // MARK: - Password

    /// Change the password. Returns nil on success, else an error message.
    func changePassword(old: String, new: String, apiClient: APIClient) async -> String? {
        isWorking = true
        defer { isWorking = false }
        do {
            let _: String = try await apiClient.put(
                "/api/auth/password",
                body: ChangePasswordRequest(oldPassword: old, newPassword: new)
            )
            return nil
        } catch {
            return Self.message(for: error, fallback: String(localized: "Couldn't change password"))
        }
    }

    // MARK: - 2FA

    func loadStatus(apiClient: APIClient) async {
        isLoadingStatus = true
        defer { isLoadingStatus = false }
        let status: TwoFactorStatus? = try? await apiClient.get("/api/auth/2fa/status")
        twoFactorEnabled = status?.enabled
    }

    /// Begin enrollment: fetch the one-time secret + QR.
    func beginSetup(apiClient: APIClient) async {
        isEnrolling = true
        enrollError = nil
        defer { isEnrolling = false }
        do {
            setup = try await apiClient.post("/api/auth/2fa/setup")
        } catch {
            enrollError = Self.message(for: error, fallback: String(localized: "Couldn't start 2FA setup"))
        }
    }

    /// Confirm enrollment with a TOTP code. Returns nil on success.
    func enable(code: String, apiClient: APIClient) async -> String? {
        isWorking = true
        defer { isWorking = false }
        do {
            let _: String = try await apiClient.post(
                "/api/auth/2fa/enable",
                body: TwoFactorEnableRequest(code: code)
            )
            twoFactorEnabled = true
            setup = nil
            return nil
        } catch {
            return Self.message(for: error, fallback: String(localized: "Invalid code, try again"))
        }
    }

    /// Disable 2FA (requires current password). Returns nil on success.
    func disable(password: String, apiClient: APIClient) async -> String? {
        isWorking = true
        defer { isWorking = false }
        do {
            let _: String = try await apiClient.post(
                "/api/auth/2fa/disable",
                body: TwoFactorDisableRequest(password: password)
            )
            twoFactorEnabled = false
            return nil
        } catch {
            return Self.message(for: error, fallback: String(localized: "Couldn't disable 2FA"))
        }
    }

    // MARK: - Error mapping

    static func message(for error: Error, fallback: String) -> String {
        if case APIError.httpError(let code, let data) = error {
            if let msg = errorMessage(from: data) { return msg }
            switch code {
            case 400: return String(localized: "Incorrect password or code")
            case 401: return String(localized: "Not authorized")
            case 422: return String(localized: "Check your input and try again")
            default: break
            }
        }
        return fallback
    }

    static func errorMessage(from data: Data) -> String? {
        guard
            let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let err = obj["error"] as? [String: Any],
            let msg = err["message"] as? String
        else { return nil }
        return msg
    }
}
