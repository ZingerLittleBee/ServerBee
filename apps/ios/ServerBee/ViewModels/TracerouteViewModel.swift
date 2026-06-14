import SwiftUI

/// Drives the interactive traceroute flow: trigger a run, poll the snapshot
/// until it completes, and list recent history. Traceroute is member-allowed
/// (read-only diagnostic) but does dispatch an agent task, so the UI surfaces a
/// clear run affordance and requires the server to be online.
@MainActor
@Observable
final class TracerouteViewModel {
    var target: String = ""
    var protocolValue: TraceProtocol = .icmp

    var snapshot: TracerouteSnapshot?
    var history: [TracerouteRecordSummary] = []

    var isRunning = false
    var isLoadingHistory = false
    var errorMessage: String?

    private var pollTask: Task<Void, Never>?

    /// Max polls before giving up (2s interval → ~60s ceiling).
    private let maxPolls = 30

    func loadHistory(serverId: String, apiClient: APIClient) async {
        isLoadingHistory = true
        defer { isLoadingHistory = false }
        do {
            history = try await apiClient.get("/api/servers/\(serverId)/traceroute?limit=20")
        } catch {
            AppLog.viewModel.error("Traceroute history failed: \(String(describing: error), privacy: .public)")
        }
    }

    /// Trigger a new traceroute and begin polling for results.
    func run(serverId: String, apiClient: APIClient) async {
        let trimmed = target.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }

        pollTask?.cancel()
        errorMessage = nil
        snapshot = nil
        isRunning = true

        do {
            let response: TriggerTracerouteResponse = try await apiClient.post(
                "/api/servers/\(serverId)/traceroute",
                body: TriggerTracerouteRequest(target: trimmed, protocolValue: protocolValue)
            )
            startPolling(serverId: serverId, requestId: response.requestId, apiClient: apiClient)
        } catch {
            isRunning = false
            errorMessage = friendlyMessage(for: error)
        }
    }

    /// Open a completed/in-progress run from history.
    func open(serverId: String, requestId: String, apiClient: APIClient) async {
        pollTask?.cancel()
        errorMessage = nil
        snapshot = nil
        isRunning = true
        startPolling(serverId: serverId, requestId: requestId, apiClient: apiClient)
    }

    func cancel() {
        pollTask?.cancel()
        isRunning = false
    }

    private func startPolling(serverId: String, requestId: String, apiClient: APIClient) {
        pollTask = Task { [weak self] in
            guard let self else { return }
            var attempts = 0
            while !Task.isCancelled {
                attempts += 1
                do {
                    let snap: TracerouteSnapshot = try await apiClient.get(
                        "/api/servers/\(serverId)/traceroute/\(requestId)"
                    )
                    self.snapshot = snap
                    if snap.completed || snap.error != nil {
                        self.isRunning = false
                        await self.loadHistory(serverId: serverId, apiClient: apiClient)
                        return
                    }
                } catch {
                    // Snapshot may 404 briefly before the agent reports; keep trying.
                    if attempts >= self.maxPolls {
                        self.isRunning = false
                        self.errorMessage = self.friendlyMessage(for: error)
                        return
                    }
                }
                if attempts >= self.maxPolls {
                    self.isRunning = false
                    if self.snapshot?.completed != true {
                        self.errorMessage = String(localized: "Traceroute timed out")
                    }
                    return
                }
                try? await Task.sleep(for: .seconds(2))
            }
        }
    }

    private func friendlyMessage(for error: Error) -> String {
        if case APIError.httpError(let code, _) = error {
            if code == 409 || code == 503 { return String(localized: "Server is offline") }
            if code == 403 { return String(localized: "Not permitted") }
        }
        return String(localized: "Traceroute failed")
    }
}
