import SwiftUI

/// Backs the signed-in devices screen. The current device is identified by
/// matching `installation_id` against this install's `InstallationID`, so no
/// token decoding is needed.
@MainActor
@Observable
final class DevicesViewModel {
    private(set) var devices: [MobileDevice] = []
    var isLoading = false
    var loadError: String?
    var actionError: String?

    let currentInstallationId = InstallationID.getOrCreate()

    func isCurrent(_ device: MobileDevice) -> Bool {
        device.installationId == currentInstallationId
    }

    func load(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        loadError = nil
        do {
            devices = try await apiClient.get("/api/mobile/auth/devices")
        } catch {
            loadError = String(localized: "Couldn't load devices")
        }
    }

    func revoke(id: String, apiClient: APIClient) async {
        actionError = nil
        do {
            let _: String = try await apiClient.delete("/api/mobile/auth/devices/\(id)")
            devices.removeAll { $0.id == id }
        } catch {
            actionError = String(localized: "Couldn't sign out that device")
        }
    }
}
