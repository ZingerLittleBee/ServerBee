import SwiftUI

/// Loads a single service monitor with its latest record and check history, and
/// (for admins) triggers an out-of-schedule check or toggles the monitor.
@MainActor
@Observable
final class ServiceMonitorDetailViewModel {
    var detail: MonitorWithRecord?
    var records: [ServiceMonitorRecord] = []
    var isLoading = false
    var isChecking = false
    var errorMessage: String?

    func load(monitorId: String, apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        detail = try? await apiClient.get("/api/service-monitors/\(monitorId)")
        if let recs: [ServiceMonitorRecord] = try? await apiClient.get("/api/service-monitors/\(monitorId)/records?limit=50") {
            records = recs
        }
    }

    /// Trigger an immediate check (admin-only). Refreshes on success.
    func runCheck(monitorId: String, apiClient: APIClient) async {
        isChecking = true
        errorMessage = nil
        defer { isChecking = false }
        do {
            let _: ServiceMonitorRecord = try await apiClient.post("/api/service-monitors/\(monitorId)/check")
            await load(monitorId: monitorId, apiClient: apiClient)
        } catch {
            errorMessage = DockerViewModel.unavailableText(for: error)
        }
    }

    /// Uptime over the loaded record window (0...1), or nil if no records.
    var recentUptime: Double? {
        guard !records.isEmpty else { return nil }
        let ok = records.filter(\.success).count
        return Double(ok) / Double(records.count)
    }
}
