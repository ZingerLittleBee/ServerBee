import Foundation

/// Captured Server credentials and registration bodies must never leave the
/// configured endpoint through redirects, including same-origin redirects.
/// A rejected redirect remains a 3xx response for the caller's retry policy.
enum ServerHTTPTransport {
    static func data(for request: URLRequest, session: URLSession = .shared) async throws -> (Data, URLResponse) {
        try await session.data(for: request, delegate: ServerNoRedirects())
    }
}

private final class ServerNoRedirects: NSObject, URLSessionTaskDelegate {
    func urlSession(
        _ session: URLSession, task: URLSessionTask,
        willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest,
        completionHandler: @escaping @Sendable (URLRequest?) -> Void
    ) {
        completionHandler(nil)
    }
}
