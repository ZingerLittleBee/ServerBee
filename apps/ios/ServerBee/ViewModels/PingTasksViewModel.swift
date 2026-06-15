import SwiftUI

/// Admin management of ping tasks: list, create, edit, toggle, delete.
/// Mirrors the CRUD-VM pattern used by `ServerGroupsViewModel`. The list route
/// is readable by members; all writes are admin-only (enforced server-side).
@MainActor
@Observable
final class PingTasksViewModel {
    private(set) var tasks: [PingTask] = []
    var isLoading = false
    var loadError: String?
    var actionError: String?

    func load(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            tasks = try await apiClient.get("/api/ping-tasks")
            loadError = nil
        } catch {
            loadError = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Failed to load ping tasks.")
            )
        }
    }

    /// Returns a localized error string on failure, nil on success.
    func create(_ request: CreatePingTaskRequest, apiClient: APIClient) async -> String? {
        actionError = nil
        do {
            let created: PingTask = try await apiClient.post("/api/ping-tasks", body: request)
            tasks.insert(created, at: 0)
            return nil
        } catch {
            return fail(error, fallback: String(localized: "Couldn't create ping task."))
        }
    }

    func update(id: String, _ request: UpdatePingTaskRequest, apiClient: APIClient) async -> String? {
        actionError = nil
        do {
            let updated: PingTask = try await apiClient.put("/api/ping-tasks/\(id)", body: request)
            if let index = tasks.firstIndex(where: { $0.id == id }) {
                tasks[index] = updated
            }
            return nil
        } catch {
            return fail(error, fallback: String(localized: "Couldn't update ping task."))
        }
    }

    /// Flip the enabled flag via a `{enabled}`-only PUT.
    func setEnabled(_ task: PingTask, enabled: Bool, apiClient: APIClient) async {
        _ = await update(id: task.id, UpdatePingTaskRequest(enabled: enabled), apiClient: apiClient)
    }

    func delete(id: String, apiClient: APIClient) async {
        actionError = nil
        do {
            let _: String = try await apiClient.delete("/api/ping-tasks/\(id)")
            tasks.removeAll { $0.id == id }
        } catch {
            actionError = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Couldn't delete ping task.")
            )
        }
    }

    private func fail(_ error: Error, fallback: String) -> String {
        let message = AccountSecurityViewModel.message(for: error, fallback: fallback)
        actionError = message
        return message
    }
}
