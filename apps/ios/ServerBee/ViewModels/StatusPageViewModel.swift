import SwiftUI

/// Admin config of the public status page (`/api/status-page`, singleton).
/// GET is readable by members; PUT is admin-only (enforced server-side).
@MainActor
@Observable
final class StatusPageViewModel {
    private(set) var config: StatusPageConfig?
    var isLoading = false
    var loadError: String?
    var actionError: String?

    func load(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            config = try await apiClient.get("/api/status-page")
            loadError = nil
        } catch {
            loadError = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Failed to load status page settings.")
            )
        }
    }

    /// Returns a localized error string on failure, nil on success.
    func save(_ request: UpdateStatusPageRequest, apiClient: APIClient) async -> String? {
        actionError = nil
        do {
            config = try await apiClient.put("/api/status-page", body: request)
            return nil
        } catch {
            let message = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Couldn't save status page settings.")
            )
            actionError = message
            return message
        }
    }
}
