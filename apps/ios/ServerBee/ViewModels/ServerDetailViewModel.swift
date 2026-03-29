import SwiftUI

@MainActor
@Observable
final class ServerDetailViewModel {
    var server: ServerStatus?
    var records: [MetricRecord] = []
    var isLoading = false
    var selectedRange = "1h"

    /// Set the server from the parent list (avoids a separate network fetch).
    func setServer(_ server: ServerStatus) {
        self.server = server
    }

    /// Fetch the server detail individually if not already provided.
    func fetchDetail(serverId: String, apiClient: APIClient) async {
        guard server == nil else { return }
        do {
            server = try await apiClient.get("/api/servers/\(serverId)")
        } catch {
            print("[ServerDetail] Fetch failed: \(error)")
        }
    }

    /// Fetch historical metric records for the given server and time range.
    func fetchRecords(serverId: String, range: String, apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            records = try await apiClient.get("/api/servers/\(serverId)/records?range=\(range)")
        } catch {
            print("[ServerDetail] Records fetch failed: \(error)")
        }
    }
}
