import SwiftUI

@MainActor
@Observable
final class AlertDetailViewModel {
    var detail: MobileAlertDetail?
    var isLoading = false
    var errorMessage: String?

    func fetchDetail(alertKey: String, apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            detail = try await apiClient.get("/api/alert-events/\(alertKey)")
        } catch {
            errorMessage = String(localized: "Alert not found")
        }
    }
}
