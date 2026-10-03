import SwiftUI

/// Push targets are verified against current authenticated resources before
/// showing either the security feed or event detail. Cached login roles do not
/// authorize this destination.
struct SecurityNotificationDetailView: View {
    let serverId: String
    let eventId: String
    @Environment(\.apiClient) private var apiClient
    @State private var model = SecurityNotificationDetailModel()
    @State private var selected: SecurityEvent?

    var body: some View {
        Group {
            if model.isLoading {
                ProgressView()
            } else if let server = model.server {
                ServerSecuritySection(serverId: server.id)
            } else {
                ContentUnavailableView("Security notification unavailable", systemImage: "shield.slash", description:
                    Text("The target may have been removed, or your account no longer has access."))
            }
        }
        .navigationTitle(model.server?.name ?? String(localized: "Security"))
        .task {
            await model.load(serverId: serverId, eventId: eventId, apiClient: apiClient)
            selected = model.event.map(SecurityEvent.init(dto:))
        }
        .sheet(item: $selected) { event in
            SecurityEventDetailView(event: event) { selected = nil }
        }
    }
}

@MainActor
@Observable
final class SecurityNotificationDetailModel {
    private(set) var server: ServerConfig?
    private(set) var event: SecurityEventDto?
    private(set) var isLoading = true

    func load(serverId: String, eventId: String, apiClient: APIClient) async {
        server = nil
        event = nil
        guard let context = apiClient.captureContext() else { isLoading = false; return }
        await load(target: SecurityNotificationTarget(serverId: serverId, eventId: eventId, userId: context.userId),
                   loadUser: { try await apiClient.get("/api/auth/me") },
                   loadServer: { try await apiClient.get("/api/servers/\(serverId)") },
                   loadEvent: { try await apiClient.get("/api/security/events/\(eventId)") },
                   isCurrent: { apiClient.isCurrent(context) })
    }

    /// Thin authenticated HTTP seam; policy and target matching stay real in tests.
    func load(target: SecurityNotificationTarget,
              loadUser: () async throws -> CurrentUserResponse,
              loadServer: () async throws -> ServerConfig,
              loadEvent: () async throws -> SecurityEventDto,
              isCurrent: () -> Bool) async {
        isLoading = true
        server = nil
        event = nil
        defer { isLoading = false }
        do {
            let owner = try await loadUser()
            guard owner.userId == target.userId, owner.role == "admin", !owner.mustChangePassword, isCurrent() else { return }
            let targetServer = try await loadServer()
            let targetEvent = try await loadEvent()
            // Recheck role after resource requests, including a mid-load downgrade.
            let currentOwner = try await loadUser()
            guard currentOwner.userId == target.userId, currentOwner.role == "admin", !currentOwner.mustChangePassword, isCurrent(),
                  targetServer.id == target.serverId, targetEvent.id == target.eventId, targetEvent.serverId == target.serverId else { return }
            server = targetServer
            event = targetEvent
        } catch {
            // A missing, forbidden or unreachable resource leaves a dismissible
            // unavailable view, with no decrypted text or raw target fallback.
        }
    }
}

struct SecurityNotificationTarget {
    let serverId: String
    let eventId: String
    let userId: String
}
