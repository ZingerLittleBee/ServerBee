import SwiftUI

/// Admin management of the global IP-quality config: the unlock-service catalog
/// (enable/disable, delete custom) and the check interval. List/get are
/// member-readable; all writes are admin-only (enforced server-side).
@MainActor
@Observable
final class IpQualityConfigViewModel {
    private(set) var services: [UnlockService] = []
    private(set) var setting: IpQualitySettingModel?
    var isLoading = false
    var loadError: String?
    var actionError: String?

    /// Services grouped by category, each group sorted by popularity desc.
    var groupedServices: [(category: String, services: [UnlockService])] {
        let groups = Dictionary(grouping: services, by: \.categoryLabel)
        return groups.keys.sorted().map { key in
            (key, groups[key]?.sorted { ($0.popularity ?? 0) > ($1.popularity ?? 0) } ?? [])
        }
    }

    func load(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            async let servicesCall: [UnlockService] = apiClient.get("/api/ip-quality/services")
            async let settingCall: IpQualitySettingModel = apiClient.get("/api/ip-quality/settings")
            services = try await servicesCall
            setting = try await settingCall
            loadError = nil
        } catch {
            loadError = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Failed to load IP quality config.")
            )
        }
    }

    /// Flip the enabled flag via an `{enabled}`-only PUT, optimistic on the row.
    func setEnabled(_ service: UnlockService, enabled: Bool, apiClient: APIClient) async {
        actionError = nil
        do {
            let updated: UnlockService = try await apiClient.put(
                "/api/ip-quality/services/\(service.id)", body: UpdateUnlockServiceRequest(enabled: enabled)
            )
            if let index = services.firstIndex(where: { $0.id == service.id }) {
                services[index] = updated
            }
        } catch {
            actionError = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Couldn't update service.")
            )
        }
    }

    func delete(_ service: UnlockService, apiClient: APIClient) async {
        actionError = nil
        do {
            let _: String = try await apiClient.delete("/api/ip-quality/services/\(service.id)")
            services.removeAll { $0.id == service.id }
        } catch {
            actionError = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Couldn't delete service.")
            )
        }
    }

    func saveSetting(checkIntervalHours: Int, apiClient: APIClient) async -> String? {
        actionError = nil
        do {
            setting = try await apiClient.put(
                "/api/ip-quality/settings", body: UpdateIpQualitySettingRequest(checkIntervalHours: checkIntervalHours)
            )
            return nil
        } catch {
            let message = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Couldn't save settings.")
            )
            actionError = message
            return message
        }
    }
}
