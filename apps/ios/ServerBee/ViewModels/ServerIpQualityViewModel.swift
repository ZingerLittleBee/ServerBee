import SwiftUI

/// Backs the server detail "IP Quality" section: the egress IP reputation
/// snapshot plus per-service unlock results. Rechecking is an admin action that
/// dispatches an async agent job; we poll the GET endpoint a few times to pick
/// up the fresh result.
@MainActor
@Observable
final class ServerIpQualityViewModel {
    var data: ServerIpQualityData?
    var serviceNames: [String: String] = [:]
    var isLoading = false
    var isChecking = false
    var loadError: String?
    var checkError: String?

    private var hasLoaded = false

    func loadIfNeeded(serverId: String, apiClient: APIClient) async {
        guard !hasLoaded else { return }
        await reload(serverId: serverId, apiClient: apiClient)
    }

    func reload(serverId: String, apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false; hasLoaded = true }
        loadError = nil

        async let dataResult = fetchData(serverId: serverId, apiClient: apiClient)
        async let servicesResult = fetchServices(apiClient: apiClient)
        let (d, services) = await (dataResult, servicesResult)

        if !services.isEmpty {
            serviceNames = Dictionary(services.map { ($0.id, $0.name) }, uniquingKeysWith: { a, _ in a })
        }
        if let d {
            data = d
        } else if data == nil {
            loadError = String(localized: "Couldn't load IP quality")
        }
    }

    /// Trigger a recheck (admin) and poll for the refreshed snapshot.
    func recheck(serverId: String, apiClient: APIClient) async {
        guard !isChecking else { return }
        isChecking = true
        checkError = nil
        defer { isChecking = false }

        do {
            // Endpoint returns a bare "ok" string.
            let _: String = try await apiClient.post("/api/ip-quality/servers/\(serverId)/check")
        } catch {
            checkError = friendlyMessage(for: error)
            return
        }

        // Poll for an updated snapshot (the agent runs the check asynchronously).
        let before = data?.ipQuality?.checkedAt
        for _ in 0..<6 {
            try? await Task.sleep(for: .seconds(2))
            if let fresh = await fetchData(serverId: serverId, apiClient: apiClient) {
                data = fresh
                if fresh.ipQuality?.checkedAt != before { return }
            }
        }
    }

    private func fetchData(serverId: String, apiClient: APIClient) async -> ServerIpQualityData? {
        do {
            return try await apiClient.get("/api/ip-quality/servers/\(serverId)")
        } catch {
            AppLog.viewModel.error("IP quality fetch failed: \(String(describing: error), privacy: .public)")
            return nil
        }
    }

    private func fetchServices(apiClient: APIClient) async -> [UnlockService] {
        do {
            return try await apiClient.get("/api/ip-quality/services")
        } catch {
            return []
        }
    }

    private func friendlyMessage(for error: Error) -> String {
        if case APIError.httpError(let code, let data) = error {
            if code == 409 {
                // The server returns a specific blame message for capability gating.
                if let msg = Self.errorMessage(from: data) { return msg }
                return String(localized: "IP quality check is unavailable")
            }
            if code == 403 { return String(localized: "Admin permission required") }
        }
        return String(localized: "Recheck failed")
    }

    /// Best-effort extraction of `error.message` from an error response body.
    private static func errorMessage(from data: Data) -> String? {
        guard
            let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let err = obj["error"] as? [String: Any],
            let msg = err["message"] as? String
        else { return nil }
        return msg
    }
}
