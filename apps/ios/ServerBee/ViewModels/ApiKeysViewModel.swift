import SwiftUI

/// Backs the API keys screen. Member-ok: every user manages their own keys.
@MainActor
@Observable
final class ApiKeysViewModel {
    private(set) var keys: [ApiKey] = []
    var isLoading = false
    var loadError: String?
    var actionError: String?

    /// The plaintext key from the most recent create — shown once, then cleared.
    var revealedKey: ApiKey?

    func load(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        loadError = nil
        do {
            keys = try await apiClient.get("/api/auth/api-keys")
        } catch {
            loadError = String(localized: "Couldn't load API keys")
        }
    }

    /// Create a key. On success the plaintext is surfaced via `revealedKey`.
    /// Returns nil on success, else an error message.
    func create(name: String, apiClient: APIClient) async -> String? {
        actionError = nil
        do {
            let created: ApiKey = try await apiClient.post(
                "/api/auth/api-keys",
                body: CreateApiKeyRequest(name: name)
            )
            revealedKey = created
            // Insert a copy without the plaintext into the list.
            var listItem = created
            listItem.key = nil
            keys.insert(listItem, at: 0)
            return nil
        } catch {
            return AccountSecurityViewModel.message(for: error, fallback: String(localized: "Couldn't create key"))
        }
    }

    func delete(id: String, apiClient: APIClient) async {
        actionError = nil
        do {
            let _: String = try await apiClient.delete("/api/auth/api-keys/\(id)")
            keys.removeAll { $0.id == id }
        } catch {
            actionError = String(localized: "Couldn't revoke key")
        }
    }
}
