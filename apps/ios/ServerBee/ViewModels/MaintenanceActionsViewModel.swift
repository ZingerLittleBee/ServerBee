import SwiftUI

/// Admin actions on maintenance windows: create, edit, and delete. Mirrors
/// `IncidentActionsViewModel`. Both the routes and this UI are admin-only.
@MainActor
@Observable
final class MaintenanceActionsViewModel {
    var isWorking = false
    var errorMessage: String?

    func create(_ request: CreateMaintenanceRequest, apiClient: APIClient) async -> Bool {
        await perform { let _: Maintenance = try await apiClient.post("/api/maintenances", body: request) }
    }

    func update(id: String, _ request: UpdateMaintenanceRequest, apiClient: APIClient) async -> Bool {
        await perform { let _: Maintenance = try await apiClient.put("/api/maintenances/\(id)", body: request) }
    }

    func delete(id: String, apiClient: APIClient) async -> Bool {
        await perform { let _: String = try await apiClient.delete("/api/maintenances/\(id)") }
    }

    private func perform(_ action: () async throws -> Void) async -> Bool {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            try await action()
            return true
        } catch {
            errorMessage = DockerViewModel.unavailableText(for: error)
            return false
        }
    }
}
