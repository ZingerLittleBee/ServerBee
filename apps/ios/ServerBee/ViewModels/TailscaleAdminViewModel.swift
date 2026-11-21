import Foundation
import Observation

/// Drives `TailscaleAdminView`: control-plane settings (`/api/tailscale/config`),
/// connection verification (`/api/tailscale/test`), and live Tailnet inventory
/// (`/api/tailscale/overview`).
@MainActor
@Observable
final class TailscaleAdminViewModel {
    private(set) var config: TailscaleAdminConfigResponse?
    private(set) var overview: TailnetOverviewResponse?
    private(set) var testResult: TailscaleConnectionTestResponse?

    var isLoading = false
    var isSaving = false
    var isTesting = false
    var loadError: String?
    var actionError: String?
    var saveSucceeded = false

    /// Devices sorted with linked ServerBee servers and online nodes first.
    var sortedDevices: [TailnetDeviceItem] {
        guard let devices = overview?.devices else { return [] }
        return devices.sorted { lhs, rhs in
            let lhsLinked = lhs.matchedServerId != nil
            let rhsLinked = rhs.matchedServerId != nil
            if lhsLinked != rhsLinked { return lhsLinked && !rhsLinked }
            if lhs.online != rhs.online { return lhs.online && !rhs.online }
            return lhs.cleanName.localizedCaseInsensitiveCompare(rhs.cleanName) == .orderedAscending
        }
    }

    func load(isAdmin: Bool, apiClient: APIClient) async {
        isLoading = true
        loadError = nil
        defer { isLoading = false }
        do {
            if isAdmin {
                async let configReq: TailscaleAdminConfigResponse = apiClient.get("/api/tailscale/config")
                async let overviewReq: TailnetOverviewResponse = apiClient.get("/api/tailscale/overview")
                let (loadedConfig, loadedOverview) = try await (configReq, overviewReq)
                config = loadedConfig
                overview = loadedOverview
            } else {
                overview = try await apiClient.get("/api/tailscale/overview")
            }
        } catch {
            loadError = AccountSecurityViewModel.message(
                for: error,
                fallback: String(localized: "Failed to load Tailscale settings.")
            )
        }
    }

    func refreshOverview(apiClient: APIClient) async {
        do {
            overview = try await apiClient.get("/api/tailscale/overview")
            actionError = nil
        } catch {
            actionError = AccountSecurityViewModel.message(
                for: error,
                fallback: String(localized: "Failed to refresh Tailnet overview.")
            )
        }
    }

    @discardableResult
    func save(_ request: UpdateTailscaleConfigRequest, apiClient: APIClient) async -> Bool {
        isSaving = true
        actionError = nil
        saveSucceeded = false
        defer { isSaving = false }
        do {
            let updated: TailscaleAdminConfigResponse = try await apiClient.put(
                "/api/tailscale/config",
                body: request
            )
            config = updated
            if let refreshed: TailnetOverviewResponse = try? await apiClient.get("/api/tailscale/overview") {
                overview = refreshed
            }
            saveSucceeded = true
            return true
        } catch {
            actionError = AccountSecurityViewModel.message(
                for: error,
                fallback: String(localized: "Couldn't save Tailscale settings.")
            )
            return false
        }
    }

    @discardableResult
    func testConnection(
        _ request: TestTailscaleConfigRequest,
        apiClient: APIClient
    ) async -> TailscaleConnectionTestResponse? {
        isTesting = true
        actionError = nil
        testResult = nil
        defer { isTesting = false }
        do {
            let result: TailscaleConnectionTestResponse = try await apiClient.post(
                "/api/tailscale/test",
                body: request
            )
            testResult = result
            return result
        } catch {
            actionError = AccountSecurityViewModel.message(
                for: error,
                fallback: String(localized: "Connection test failed.")
            )
            return nil
        }
    }
}
