import SwiftUI

/// Backs the admin user-management screen. Admin-only: the server enforces
/// `require_admin` and blocks demoting/deleting the last admin.
@MainActor
@Observable
final class UsersViewModel {
    private(set) var users: [AdminUser] = []
    var isLoading = false
    var loadError: String?
    var actionError: String?

    func load(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        loadError = nil
        do {
            users = try await apiClient.get("/api/users")
        } catch {
            loadError = String(localized: "Couldn't load users")
        }
    }

    /// Returns nil on success, else an error message.
    func create(username: String, password: String, role: String, apiClient: APIClient) async -> String? {
        do {
            let created: AdminUser = try await apiClient.post(
                "/api/users",
                body: CreateUserRequest(username: username, password: password, role: role)
            )
            users.append(created)
            users.sort { $0.createdAt < $1.createdAt }
            return nil
        } catch {
            return AccountSecurityViewModel.message(for: error, fallback: String(localized: "Couldn't create user"))
        }
    }

    func setRole(id: String, role: String, apiClient: APIClient) async -> String? {
        do {
            let updated: AdminUser = try await apiClient.put("/api/users/\(id)", body: UpdateUserRequest(role: role, password: nil))
            if let idx = users.firstIndex(where: { $0.id == id }) { users[idx] = updated }
            return nil
        } catch {
            return AccountSecurityViewModel.message(for: error, fallback: String(localized: "Couldn't update role"))
        }
    }

    func resetPassword(id: String, password: String, apiClient: APIClient) async -> String? {
        do {
            let _: AdminUser = try await apiClient.put("/api/users/\(id)", body: UpdateUserRequest(role: nil, password: password))
            return nil
        } catch {
            return AccountSecurityViewModel.message(for: error, fallback: String(localized: "Couldn't reset password"))
        }
    }

    func delete(id: String, apiClient: APIClient) async {
        actionError = nil
        do {
            let _: String = try await apiClient.delete("/api/users/\(id)")
            users.removeAll { $0.id == id }
        } catch {
            actionError = AccountSecurityViewModel.message(for: error, fallback: String(localized: "Couldn't delete user"))
        }
    }
}
