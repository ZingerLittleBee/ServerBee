import SwiftUI

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
            totpCode: step == .totp ? totpCode : nil
        )

        do {
            guard let url = URL(string: "\(normalizedUrl)/api/mobile/auth/login") else {
                errorMessage = String(localized: "Invalid server URL.")
                isLoading = false
                return
            }

            var request = URLRequest(url: url)
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")

            let encoder = JSONEncoder()
            encoder.keyEncodingStrategy = .convertToSnakeCase
            request.httpBody = try encoder.encode(loginRequest)

            let (data, response) = try await URLSession.shared.data(for: request)
            let httpResponse = response as! HTTPURLResponse

            switch httpResponse.statusCode {
            case 200:
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let tokenResponse = try decoder.decode(
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
            errorMessage = String(localized: "Connection failed. Please check your server URL.")
        }

        isLoading = false
    }

    func goBackToCredentials() {
        step = .credentials
        totpCode = ""
        errorMessage = ""
    }
}
