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
            let response: ApiResponse<ServerStatus> = try await apiClient.get("/api/servers/\(serverId)")
            server = response.data
        } catch {
            print("[ServerDetail] Fetch failed: \(error)")
        }
    }

    /// Fetch historical metric records for the given server and time range.
    func fetchRecords(serverId: String, range: String, apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            let response: ApiResponse<[MetricRecord]> = try await apiClient.get(
                "/api/servers/\(serverId)/records?range=\(range)"
            )
            records = response.data
        } catch {
            print("[ServerDetail] Records fetch failed: \(error)")
        }
    }
}
