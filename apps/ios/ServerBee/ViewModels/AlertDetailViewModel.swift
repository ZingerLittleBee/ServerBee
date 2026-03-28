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
            let response: ApiResponse<MobileAlertDetail> = try await apiClient.get("/api/mobile/alerts/\(alertKey)")
            detail = response.data
        } catch {
            errorMessage = String(localized: "Alert not found")
        }
    }
}
