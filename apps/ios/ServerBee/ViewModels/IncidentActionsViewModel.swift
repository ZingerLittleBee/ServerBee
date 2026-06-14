import SwiftUI

/// Admin actions on incidents: create a new incident and append a status update
/// (which also advances the incident's status / resolves it server-side).
@MainActor
@Observable
final class IncidentActionsViewModel {
    var isWorking = false
    var errorMessage: String?

    /// Create an incident. Returns true on success.
    func create(title: String, severity: IncidentSeverity, isPublic: Bool, apiClient: APIClient) async -> Bool {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let _: Incident = try await apiClient.post(
                "/api/incidents",
                body: CreateIncidentRequest(title: title, severity: severity.rawValue, isPublic: isPublic)
            )
            return true
        } catch {
            errorMessage = DockerViewModel.unavailableText(for: error)
            return false
        }
    }

    /// Append a status update (advances the incident status; `resolved` resolves it).
    func addUpdate(incidentId: String, status: IncidentStatus, message: String, apiClient: APIClient) async -> Bool {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let _: IncidentUpdate = try await apiClient.post(
                "/api/incidents/\(incidentId)/updates",
                body: CreateIncidentUpdateRequest(status: status.rawValue, message: message)
            )
            return true
        } catch {
            errorMessage = DockerViewModel.unavailableText(for: error)
            return false
        }
    }

    /// Delete an incident. Returns true on success.
    func delete(incidentId: String, apiClient: APIClient) async -> Bool {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let _: String = try await apiClient.delete("/api/incidents/\(incidentId)")
            return true
        } catch {
            errorMessage = DockerViewModel.unavailableText(for: error)
            return false
        }
    }
}
