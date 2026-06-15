import SwiftUI

/// Admin management of server groups: list, create, rename, delete.
/// Mirrors the CRUD-VM pattern used by ApiKeysViewModel / UsersViewModel.
@MainActor
@Observable
final class ServerGroupsViewModel {
    private(set) var groups: [ServerGroup] = []
    var isLoading = false
    var loadError: String?
    var actionError: String?

    func load(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            groups = try await apiClient.get("/api/server-groups")
            loadError = nil
        } catch {
            loadError = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Failed to load groups.")
            )
        }
    }

    /// Returns a localized error string on failure, nil on success.
    func create(name: String, apiClient: APIClient) async -> String? {
        actionError = nil
        do {
            let created: ServerGroup = try await apiClient.post(
                "/api/server-groups", body: CreateGroupRequest(name: name)
            )
            groups.append(created)
            return nil
        } catch {
            let message = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Couldn't create group.")
            )
            actionError = message
            return message
        }
    }

    func rename(id: String, name: String, apiClient: APIClient) async -> String? {
        actionError = nil
        do {
            let updated: ServerGroup = try await apiClient.put(
                "/api/server-groups/\(id)", body: UpdateGroupRequest(name: name, weight: nil)
            )
            if let index = groups.firstIndex(where: { $0.id == id }) {
                groups[index] = updated
            }
            return nil
        } catch {
            let message = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Couldn't rename group.")
            )
            actionError = message
            return message
        }
    }

    func delete(id: String, apiClient: APIClient) async {
        actionError = nil
        do {
            let _: String = try await apiClient.delete("/api/server-groups/\(id)")
            groups.removeAll { $0.id == id }
        } catch {
            actionError = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Couldn't delete group.")
            )
        }
    }
}
