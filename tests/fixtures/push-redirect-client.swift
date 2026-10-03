import Foundation
import Security

/// Test-only trust boundary for the temporary localhost certificate. Production
/// ServerHTTPTransport uses the system trust policy and URLSession.shared.
private final class FixtureTrust: NSObject, URLSessionDelegate {
    func urlSession(
        _ session: URLSession, didReceive challenge: URLAuthenticationChallenge,
        completionHandler: @escaping @Sendable (URLSession.AuthChallengeDisposition, URLCredential?) -> Void
    ) {
        guard challenge.protectionSpace.host == "localhost",
              challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodServerTrust,
              let trust = challenge.protectionSpace.serverTrust else {
            completionHandler(.performDefaultHandling, nil)
            return
        }
        completionHandler(.useCredential, URLCredential(trust: trust))
    }
}

@main
struct RedirectBoundaryClient {
    static func main() async throws {
        let session = URLSession(configuration: .ephemeral, delegate: FixtureTrust(), delegateQueue: nil)
        defer { session.invalidateAndCancel() }
        let base = CommandLine.arguments[1]
        for status in [307, 308] {
            for target in ["http", "https", "same-origin"] {
                guard let url = URL(string: "\(base)/\(status)/\(target)") else { throw URLError(.badURL) }
                var request = URLRequest(url: url)
                request.httpMethod = "POST"
                request.timeoutInterval = 5
                request.setValue("Bearer synthetic-access", forHTTPHeaderField: "Authorization")
                request.setValue("application/json", forHTTPHeaderField: "Content-Type")
                request.httpBody = Data(#"{"content_key":"synthetic-content-key","refresh_token":"synthetic-refresh","revocation_token":"synthetic-revocation"}"#.utf8)
                let (_, response) = try await ServerHTTPTransport.data(for: request, session: session)
                guard (response as? HTTPURLResponse)?.statusCode == status else { throw URLError(.badServerResponse) }
            }
        }
        print("Native URLSession redirect cases: 6 passed")
    }
}
