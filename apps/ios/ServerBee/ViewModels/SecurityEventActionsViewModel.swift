import SwiftUI

/// Admin action on a security event: permanent delete. The server returns
/// `{ "data": bool }` and gates the route behind the admin role; the client
/// gate is UX only.
@MainActor
@Observable
final class SecurityEventActionsViewModel {
    var isWorking = false
    var errorMessage: String?

    /// Delete a security event. Returns true on success.
    func delete(id: String, apiClient: APIClient) async -> Bool {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let _: Bool = try await apiClient.delete("/api/security/events/\(id)")
            return true
        } catch {
            errorMessage = DockerViewModel.unavailableText(for: error)
            return false
        }
    }
}
