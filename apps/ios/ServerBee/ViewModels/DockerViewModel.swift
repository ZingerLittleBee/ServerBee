import SwiftUI

/// Drives the Docker section of the server detail screen: containers, per-container
/// stats, system info, events, networks, volumes, and admin container actions.
///
/// Docker is gated server-side on `CAP_DOCKER`, the agent reporting the "docker"
/// feature, and the agent being online; the read endpoints return 403/404 when
/// those preconditions fail, which we surface as a friendly unavailable state.
@MainActor
@Observable
final class DockerViewModel {
    var containers: [DockerContainer] = []
    var statsById: [String: DockerContainerStats] = [:]
    var info: DockerSystemInfo?

    var events: [DockerEventInfo] = []
    var networks: [DockerNetwork] = []
    var volumes: [DockerVolume] = []

    var isLoading = false
    var hasLoaded = false
    /// Non-nil when the section can't be shown (offline, capability denied, …).
    var unavailableMessage: String?
    /// Transient error for an action (start/stop/…).
    var actionError: String?
    /// Container ids with an action in flight (disables their buttons).
    var pendingActions: Set<String> = []

    // MARK: - Load

    func load(serverId: String, apiClient: APIClient) async {
        isLoading = true
        unavailableMessage = nil
        defer { isLoading = false; hasLoaded = true }
        do {
            async let containersTask: DockerContainersResponse = apiClient.get("/api/servers/\(serverId)/docker/containers")
            async let statsTask: DockerStatsResponse = apiClient.get("/api/servers/\(serverId)/docker/stats")
            let containersResp = try await containersTask
            let statsResp = try await statsTask
            containers = containersResp.containers.sorted { $0.displayName < $1.displayName }
            statsById = Dictionary(statsResp.stats.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            // Info is best-effort: it may trigger a 30s agent round-trip.
            if let infoResp: DockerInfoResponse = try? await apiClient.get("/api/servers/\(serverId)/docker/info") {
                info = infoResp.info
            }
        } catch {
            unavailableMessage = Self.unavailableText(for: error)
        }
    }

    func refresh(serverId: String, apiClient: APIClient) async {
        await load(serverId: serverId, apiClient: apiClient)
    }

    func loadEvents(serverId: String, apiClient: APIClient) async {
        if let resp: DockerEventsResponse = try? await apiClient.get("/api/servers/\(serverId)/docker/events?limit=100") {
            events = resp.events.sorted { $0.timestamp > $1.timestamp }
        }
    }

    func loadNetworks(serverId: String, apiClient: APIClient) async {
        if let resp: DockerNetworksResponse = try? await apiClient.get("/api/servers/\(serverId)/docker/networks") {
            networks = resp.networks.sorted { $0.name < $1.name }
        }
    }

    func loadVolumes(serverId: String, apiClient: APIClient) async {
        if let resp: DockerVolumesResponse = try? await apiClient.get("/api/servers/\(serverId)/docker/volumes") {
            volumes = resp.volumes.sorted { $0.name < $1.name }
        }
    }

    // MARK: - Actions (admin-only)

    /// Run a container action. Returns true on success and reloads the list.
    @discardableResult
    func perform(_ action: DockerAction, on containerId: String, serverId: String, apiClient: APIClient) async -> Bool {
        actionError = nil
        pendingActions.insert(containerId)
        defer { pendingActions.remove(containerId) }
        do {
            let result: DockerActionResult = try await apiClient.post(
                "/api/servers/\(serverId)/docker/containers/\(containerId)/action",
                body: ContainerActionRequest(action: action)
            )
            if result.success {
                await load(serverId: serverId, apiClient: apiClient)
                return true
            }
            actionError = result.error ?? String(localized: "The action failed on the server.")
            return false
        } catch {
            actionError = Self.unavailableText(for: error)
            return false
        }
    }

    func stats(for container: DockerContainer) -> DockerContainerStats? {
        statsById[container.id]
    }

    // MARK: - Helpers

    static func unavailableText(for error: Error) -> String {
        if case APIError.httpError(let code, let data) = error {
            if let msg = AccountSecurityViewModel.errorMessage(from: data), !msg.isEmpty { return msg }
            switch code {
            case 403: return String(localized: "Docker is not enabled for this server.")
            case 404: return String(localized: "The agent is offline.")
            case 408: return String(localized: "The agent did not respond in time.")
            default: break
            }
        }
        return String(localized: "Couldn't load Docker data.")
    }
}
