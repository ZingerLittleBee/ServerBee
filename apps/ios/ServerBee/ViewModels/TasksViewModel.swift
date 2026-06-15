import SwiftUI

/// Admin management of command tasks: list, create, edit, run, toggle, delete,
/// and load per-task results. All routes are admin-only (server-enforced) and
/// run arbitrary commands on agents, so the UI guards each write with explicit
/// confirmation. Mirrors the CRUD-VM pattern used elsewhere.
@MainActor
@Observable
final class TasksViewModel {
    private(set) var tasks: [CommandTask] = []
    var isLoading = false
    var loadError: String?
    var actionError: String?

    func load(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            tasks = try await apiClient.get("/api/tasks")
            loadError = nil
        } catch {
            loadError = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Failed to load tasks.")
            )
        }
    }

    /// Returns a localized error string on failure, nil on success.
    func create(_ request: CreateTaskRequest, apiClient: APIClient) async -> String? {
        actionError = nil
        do {
            let created: CommandTask = try await apiClient.post("/api/tasks", body: request)
            tasks.insert(created, at: 0)
            return nil
        } catch {
            return fail(error, fallback: String(localized: "Couldn't create task."))
        }
    }

    func update(id: String, _ request: UpdateTaskRequest, apiClient: APIClient) async -> String? {
        actionError = nil
        do {
            let updated: CommandTask = try await apiClient.put("/api/tasks/\(id)", body: request)
            replace(updated)
            return nil
        } catch {
            return fail(error, fallback: String(localized: "Couldn't update task."))
        }
    }

    /// Flip the enabled flag (scheduled tasks) via an `{enabled}`-only PUT.
    func setEnabled(_ task: CommandTask, enabled: Bool, apiClient: APIClient) async {
        _ = await update(id: task.id, UpdateTaskRequest(enabled: enabled), apiClient: apiClient)
    }

    /// Trigger a scheduled task now. Returns nil on success, else a message
    /// (e.g. 409 already running).
    func run(id: String, apiClient: APIClient) async -> String? {
        actionError = nil
        do {
            let updated: CommandTask = try await apiClient.post("/api/tasks/\(id)/run")
            replace(updated)
            return nil
        } catch {
            return fail(error, fallback: String(localized: "Couldn't run task."))
        }
    }

    func delete(id: String, apiClient: APIClient) async {
        actionError = nil
        do {
            try await apiClient.deleteVoid("/api/tasks/\(id)")
            tasks.removeAll { $0.id == id }
        } catch {
            actionError = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Couldn't delete task.")
            )
        }
    }

    func results(id: String, apiClient: APIClient) async -> [TaskResult] {
        do {
            return try await apiClient.get("/api/tasks/\(id)/results")
        } catch {
            return []
        }
    }

    private func replace(_ task: CommandTask) {
        if let index = tasks.firstIndex(where: { $0.id == task.id }) {
            tasks[index] = task
        }
    }

    private func fail(_ error: Error, fallback: String) -> String {
        let message = AccountSecurityViewModel.message(for: error, fallback: fallback)
        actionError = message
        return message
    }
}
