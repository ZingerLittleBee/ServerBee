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
        detail = nil
        errorMessage = nil
        do {
            let refreshed: MobileAlertDetail = try await apiClient.get("/api/alert-events/\(alertKey)")
            guard !Task.isCancelled else { return }
            detail = refreshed
        } catch {
            guard !Task.isCancelled else { return }
            errorMessage = String(localized: "Alert not found")
        }
    }
}
