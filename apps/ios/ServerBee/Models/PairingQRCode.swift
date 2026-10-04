import Foundation

/// The pairing payload used by both normal login and session recovery scans.
struct PairingQRCode: Decodable, Sendable {
    let type: String
    let serverUrl: String
    let code: String

    enum CodingKeys: String, CodingKey {
        case type, code
        case serverUrl = "server_url"
    }

    static func decode(_ text: String) -> PairingQRCode? {
        guard let data = text.data(using: .utf8),
              let payload = try? JSONDecoder().decode(PairingQRCode.self, from: data),
              payload.type == "serverbee_pair", !payload.serverUrl.isEmpty, !payload.code.isEmpty else { return nil }
        return payload
    }
}
