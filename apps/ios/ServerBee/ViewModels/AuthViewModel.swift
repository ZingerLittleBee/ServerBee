import SwiftUI
import UIKit

@Observable
final class AuthViewModel {
    enum LoginStep {
        case credentials
        case totp
    }

    var step: LoginStep = .credentials
    var serverUrlInput = ""
    var username = ""
    var password = ""
    var totpCode = ""
    var isLoading = false
    var errorMessage = ""

    @MainActor
    func login(authManager: AuthManager) async {
        guard !isLoading else { return }
        isLoading = true
        errorMessage = ""
        defer { isLoading = false }

        var normalizedUrl = serverUrlInput.trimmingCharacters(in: .whitespacesAndNewlines)
        if normalizedUrl.hasSuffix("/") {
            normalizedUrl = String(normalizedUrl.dropLast())
        }
        if !normalizedUrl.hasPrefix("http://") && !normalizedUrl.hasPrefix("https://") {
            normalizedUrl = "https://\(normalizedUrl)"
        }

        let installationId = InstallationID.getOrCreate()

        let loginRequest = MobileLoginRequest(
            username: username,
            password: password,
            installationId: installationId,
            deviceName: DeviceNameProvider.current(),
            totpCode: step == .totp ? totpCode : nil
        )

        do {
            guard let url = URL(string: "\(normalizedUrl)/api/mobile/auth/login") else {
                errorMessage = String(localized: "Invalid server URL.")
                return
            }

            var request = URLRequest(url: url)
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")

            request.httpBody = try JSONEncoder.snakeCase.encode(loginRequest)

            let (data, response) = try await URLSession.shared.data(for: request)

            guard let httpResponse = response as? HTTPURLResponse else {
                errorMessage = String(localized: "Connection failed. Please check your server URL.")
                return
            }

            switch httpResponse.statusCode {
            case 200:
                let tokenResponse = try JSONDecoder.snakeCase.decode(
                    ApiResponse<MobileTokenResponse>.self, from: data
                ).data
                authManager.setServerUrl(normalizedUrl)
                authManager.handleLoginResponse(tokenResponse)

            case 401:
                errorMessage = String(localized: "Invalid credentials.")

            case 422:
                step = .totp
                errorMessage = ""

            case 429:
                errorMessage = String(localized: "Too many attempts. Please try again later.")

            default:
                errorMessage = String(localized: "Connection failed. Please check your server URL.")
            }
        } catch {
            AppLog.auth.error("Login request failed: \(String(describing: error), privacy: .public)")
            errorMessage = String(localized: "Connection failed. Please check your server URL.")
        }
    }

    func goBackToCredentials() {
        step = .credentials
        totpCode = ""
        errorMessage = ""
    }

    enum PairError: LocalizedError, Equatable {
        case invalidServerUrl
        case invalidOrExpiredCode
        case endpointNotFound
        case rateLimited
        case validation
        case transport
        case http(Int)

        var errorDescription: String? {
            switch self {
            case .invalidServerUrl:
                String(localized: "Invalid server URL in QR code.")
            case .invalidOrExpiredCode:
                String(localized: "Invalid or expired QR code. Please try again.")
            case .endpointNotFound:
                String(localized: "Pairing endpoint not found. Check server version.")
            case .rateLimited:
                String(localized: "Too many attempts. Please try again later.")
            case .validation:
                String(localized: "Invalid pairing request. Please rescan the QR code.")
            case .transport:
                String(localized: "Connection failed. Please check the server URL.")
            case let .http(code):
                String(localized: "Pairing failed (HTTP \(code)).")
            }
        }
    }

    /// Redeems a pair code obtained from a QR scan and, on success, hydrates
    /// the `AuthManager`. Throws `PairError` so the View can surface a
    /// localized message; on `200` the manager is updated and the function
    /// returns the token response.
    @MainActor
    func pair(
        serverUrl rawUrl: String,
        code: String,
        authManager: AuthManager,
        session: URLSession = .shared
    ) async throws -> MobileTokenResponse {
        var serverUrl = rawUrl.trimmingCharacters(in: .whitespacesAndNewlines)
        if serverUrl.hasSuffix("/") {
            serverUrl = String(serverUrl.dropLast())
        }
        guard let url = URL(string: "\(serverUrl)/api/mobile/auth/pair") else {
            throw PairError.invalidServerUrl
        }

        let body: [String: String] = [
            "code": code,
            "installation_id": InstallationID.getOrCreate(),
            "device_name": DeviceNameProvider.current(),
        ]

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: body)

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch {
            throw PairError.transport
        }

        guard let http = response as? HTTPURLResponse else {
            throw PairError.transport
        }

        switch http.statusCode {
        case 200:
            let tokenResponse = try JSONDecoder.snakeCase.decode(
                ApiResponse<MobileTokenResponse>.self,
                from: data
            ).data
            authManager.setServerUrl(serverUrl)
            authManager.handleLoginResponse(tokenResponse)
            return tokenResponse
        case 400:
            throw PairError.invalidOrExpiredCode
        case 404:
            throw PairError.endpointNotFound
        case 422:
            // Backend uses 422 for pair only as a missing-field validation
            // error (see crates/server/src/router/api/mobile.rs:323-381).
            // There is NO 2FA branch on /api/mobile/auth/pair, so we never
            // route this into `step = .totp` like /auth/login does.
            throw PairError.validation
        case 429:
            throw PairError.rateLimited
        default:
            throw PairError.http(http.statusCode)
        }
    }
}
