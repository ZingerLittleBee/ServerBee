import SwiftUI

/// Admin management of the global network-probe config: the target catalog
/// (preset + custom) and fleet-wide settings. List/get are member-readable;
/// all writes are admin-only (enforced server-side).
@MainActor
@Observable
final class NetworkProbeConfigViewModel {
    private(set) var targets: [NetworkProbeTarget] = []
    private(set) var setting: NetworkProbeSetting?
    var isLoading = false
    var loadError: String?
    var actionError: String?

    /// Custom (admin-created) targets, which can be edited and deleted.
    var customTargets: [NetworkProbeTarget] { targets.filter { !$0.isPreset } }
    /// Preset targets, shown read-only.
    var presetTargets: [NetworkProbeTarget] { targets.filter(\.isPreset) }

    func load(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            async let targetsCall: [NetworkProbeTarget] = apiClient.get("/api/network-probes/targets")
            async let settingCall: NetworkProbeSetting = apiClient.get("/api/network-probes/setting")
            targets = try await targetsCall
            setting = try await settingCall
            loadError = nil
        } catch {
            loadError = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Failed to load network probe config.")
            )
        }
    }

    /// Returns a localized error string on failure, nil on success.
    func createTarget(_ request: CreateProbeTargetRequest, apiClient: APIClient) async -> String? {
        actionError = nil
        do {
            let created: NetworkProbeTarget = try await apiClient.post("/api/network-probes/targets", body: request)
            targets.append(created)
            return nil
        } catch {
            return fail(error, fallback: String(localized: "Couldn't create target."))
        }
    }

    func updateTarget(id: String, _ request: UpdateProbeTargetRequest, apiClient: APIClient) async -> String? {
        actionError = nil
        do {
            let updated: NetworkProbeTarget = try await apiClient.put("/api/network-probes/targets/\(id)", body: request)
            if let index = targets.firstIndex(where: { $0.id == id }) {
                targets[index] = updated
            }
            return nil
        } catch {
            return fail(error, fallback: String(localized: "Couldn't update target."))
        }
    }

    func deleteTarget(id: String, apiClient: APIClient) async {
        actionError = nil
        do {
            let _: String = try await apiClient.delete("/api/network-probes/targets/\(id)")
            targets.removeAll { $0.id == id }
        } catch {
            actionError = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Couldn't delete target.")
            )
        }
    }

    func saveSetting(_ request: UpdateProbeSettingRequest, apiClient: APIClient) async -> String? {
        actionError = nil
        do {
            setting = try await apiClient.put("/api/network-probes/setting", body: request)
            return nil
        } catch {
            return fail(error, fallback: String(localized: "Couldn't save settings."))
        }
    }

    private func fail(_ error: Error, fallback: String) -> String {
        let message = AccountSecurityViewModel.message(for: error, fallback: fallback)
        actionError = message
        return message
    }
}
