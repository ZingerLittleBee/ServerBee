import SwiftUI

struct TaskRunTarget: Identifiable, Equatable, Sendable {
    let taskId: String
    let runId: String
    var id: String { "\(taskId):\(runId)" }

    var resultsPath: String { "/api/tasks/\(taskId)/results?run_id=\(runId)" }
    var isValid: Bool { UUID(uuidString: taskId) != nil && UUID(uuidString: runId) != nil }
}

@MainActor
@Observable
final class TaskRunResultsViewModel {
    private(set) var results: [TaskResult] = []
    private(set) var unavailable = false
    private(set) var isLoading = false

    func load(target: TaskRunTarget, apiClient: APIClient, isAdmin: Bool) async {
        results = []
        unavailable = false
        guard isAdmin, target.isValid, let context = apiClient.captureContext() else { unavailable = true; return }
        isLoading = true
        defer { isLoading = false }
        do {
            let rows: [TaskResult] = try await apiClient.get(target.resultsPath, context: context)
            guard apiClient.isCurrent(context) else { unavailable = true; return }
            guard rows.allSatisfy({ $0.taskId == target.taskId && $0.runId == target.runId }) else { unavailable = true; return }
            results = rows
        } catch { unavailable = true }
    }
}
