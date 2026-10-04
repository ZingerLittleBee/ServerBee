import Foundation

/// Account reauthentication grants cleanup only, without publishing a login.
enum SessionRecoveryClient {
    static func recover(_ original: SavedMobileAuthentication, authorization: SessionRecoveryAuthorization,
                        session: URLSession) async throws -> MobileSessionRecoveryResponse {
        guard let url = URL(string: "\(original.serverUrl)/api/mobile/auth/recover") else { throw AuthError.noServerUrl }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 15
        request.httpShouldHandleCookies = false
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder.snakeCase.encode(MobileSessionRecoveryRequest(original: original, authorization: authorization))
        let (data, response) = try await ServerHTTPTransport.data(for: request, session: session)
        guard let response = response as? HTTPURLResponse else { throw SessionRecoveryError.invalidConfirmation }
        switch response.statusCode {
        case 200:
            guard let result = try? JSONDecoder.snakeCase.decode(ApiResponse<MobileSessionRecoveryResponse>.self, from: data).data,
                  result.isValid(for: original) else { throw SessionRecoveryError.invalidConfirmation }
            return result
        case 400: throw SessionRecoveryError.invalidRecoveryCode
        case 401:
            if authorization.passwordCredentials != nil { throw AuthError.invalidCredentials }
            throw SessionRecoveryError.invalidRecoveryCode
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

    /// A scanned URL can never redirect captured credentials to another server.
    static func matchesSavedServer(_ scanned: String, saved: String) -> Bool {
        guard let scanned = normalizedServer(scanned), let saved = normalizedServer(saved) else { return false }
        return scanned == saved
    }

    private static func normalizedServer(_ raw: String) -> URLComponents? {
        guard var components = URLComponents(string: raw.trimmingCharacters(in: .whitespacesAndNewlines)),
              let scheme = components.scheme?.lowercased(), ["http", "https"].contains(scheme),
              let host = components.host, !host.isEmpty, components.user == nil, components.password == nil,
              components.query == nil, components.fragment == nil else { return nil }
        components.scheme = scheme
        components.host = host.lowercased()
        if components.port == (scheme == "https" ? 443 : 80) { components.port = nil }
        while components.path.hasSuffix("/") { components.path.removeLast() }
        return components
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
    case mismatchedQRServer
    case invalidRecoveryCode

    var errorDescription: String? {
        switch self {
        case .unsupportedServer:
            return String(localized: "This server needs an update before it can recover this session. Your saved identity has been kept.")
        case .unresolvedIdentity:
            return String(localized: "The server could not identify the original session safely. No other session was removed.")
        case .invalidConfirmation:
            return String(localized: "The server did not confirm cleanup of this identity. Your saved identity has been kept.")
        case .mismatchedQRServer:
            return String(localized: "Scan a code from the saved server and original account. Your saved identity has been kept.")
        case .invalidRecoveryCode:
            return String(localized: "This QR code or recovery authorization is invalid or expired. Scan a new code from the original account.")
        }
    }
}
