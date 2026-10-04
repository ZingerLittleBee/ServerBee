import Foundation

/// Account reauthentication grants cleanup only, without publishing a login.
enum SessionRecoveryClient {
    static func recover(_ original: SavedMobileAuthentication, username: String, password: String,
                        totpCode: String?, session: URLSession) async throws -> MobileSessionRecoveryResponse {
        guard let url = URL(string: "\(original.serverUrl)/api/mobile/auth/recover") else { throw AuthError.noServerUrl }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 15
        request.httpShouldHandleCookies = false
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder.snakeCase.encode(MobileSessionRecoveryRequest(
            username: username, password: password, totpCode: totpCode,
            expectedUserId: original.user.id, installationId: original.installationId,
            expectedSessionId: original.recoveryTargetSessionId, accessToken: original.accessToken,
            refreshToken: original.refreshToken, revocationToken: original.revocationToken))
        let (data, response) = try await ServerHTTPTransport.data(for: request, session: session)
        guard let response = response as? HTTPURLResponse else { throw SessionRecoveryError.invalidConfirmation }
        switch response.statusCode {
        case 200:
            guard let result = try? JSONDecoder.snakeCase.decode(ApiResponse<MobileSessionRecoveryResponse>.self, from: data).data,
                  result.isValid(for: original) else { throw SessionRecoveryError.invalidConfirmation }
            return result
        case 401: throw AuthError.invalidCredentials
        case 404: throw SessionRecoveryError.unsupportedServer
        case 409: throw SessionRecoveryError.unresolvedIdentity
        case 422:
            let error = try? JSONDecoder().decode(RecoveryServerError.self, from: data)
            if error?.error.message.contains("2fa_required") == true { throw AuthError.twoFactorRequired }
            throw APIError.httpError(statusCode: response.statusCode, data: data)
        case 429: throw AuthError.tooManyAttempts
        default: throw APIError.httpError(statusCode: response.statusCode, data: data)
        }
    }
}

private struct RecoveryServerError: Decodable {
    struct Detail: Decodable { let message: String }
    let error: Detail
}

enum SessionRecoveryError: Error, LocalizedError {
    case unsupportedServer
    case unresolvedIdentity
    case invalidConfirmation

    var errorDescription: String? {
        switch self {
        case .unsupportedServer:
            return String(localized: "This server needs an update before it can recover this session. Your saved identity has been kept.")
        case .unresolvedIdentity:
            return String(localized: "The server could not identify the original session safely. No other session was removed.")
        case .invalidConfirmation:
            return String(localized: "The server did not confirm cleanup of this identity. Your saved identity has been kept.")
        }
    }
}
