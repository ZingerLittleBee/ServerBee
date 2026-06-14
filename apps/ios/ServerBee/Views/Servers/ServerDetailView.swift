import SwiftUI

/// Native, segmented detail screen for a single server.
///
/// Live runtime state (status, metrics) comes from the WebSocket-backed
/// `ServersViewModel` so the screen updates in real time; static configuration
/// (capabilities, billing, kernel, agent version, enrollment) comes from a REST
/// fetch. Sections are gated on the server's *effective* capabilities and the
/// caller's role.
struct ServerDetailView: View {
    let serverId: String

    @Environment(ServersViewModel.self) private var serversViewModel
    @Environment(\.apiClient) private var apiClient
    @Environment(AuthManager.self) private var authManager
    @State private var viewModel = ServerDetailViewModel()
    @State private var section: DetailSection = .overview

    /// Allow constructing from a known live status (list navigation) or just an
    /// id (deep link / push).
    init(server: ServerStatus) {
        self.serverId = server.id
    }

    init(serverId: String) {
        self.serverId = serverId
    }

    private var live: ServerStatus? {
        serversViewModel.servers.first { $0.id == serverId }
    }

    private var isAdmin: Bool {
        authManager.user?.role.lowercased() == "admin"
    }

    /// Capability set, preferring live WS data, falling back to REST config.
    private var capabilities: CapabilitySet {
        live?.capabilitySet ?? viewModel.config?.capabilitySet ?? CapabilitySet()
    }

    private var displayName: String {
        live?.name ?? viewModel.config?.name ?? String(localized: "Server")
    }

    private var groupName: String? {
        if let live { return serversViewModel.resolvedGroupName(for: live) }
        if let gid = viewModel.config?.groupId { return serversViewModel.groupsByID[gid] }
        return nil
    }

    /// Which segmented sections to show. Traffic (usage / cost / uptime) is
    /// always available since uptime applies to every enrolled server. Network
    /// and Security are gated on the server's effective capabilities.
    private var availableSections: [DetailSection] {
        var result: [DetailSection] = [.overview, .metrics, .traffic]
        if capabilities.isEnabled(.pingICMP) || capabilities.isEnabled(.pingTCP) || capabilities.isEnabled(.pingHTTP) {
            result.append(.network)
        }
        if capabilities.isEnabled(.securityEvents) { result.append(.security) }
        return result
    }

    var body: some View {
        VStack(spacing: 0) {
            if availableSections.count > 1 {
                sectionPicker
                    .padding(.horizontal)
                    .padding(.vertical, 8)
                    .background(Color(.systemGroupedBackground))
            }
            sectionContent
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .background(Color(.systemGroupedBackground))
        .navigationTitle(displayName)
        .navigationBarTitleDisplayMode(.inline)
        .task {
            await viewModel.fetchConfig(serverId: serverId, apiClient: apiClient)
            #if DEBUG
            if let raw = UITestSupport.detailSection,
               let target = DetailSection(rawValue: raw),
               availableSections.contains(target) {
                section = target
            }
            #endif
        }
        .onChange(of: availableSections) { _, sections in
            // If the active section disappears (caps changed), fall back.
            if !sections.contains(section) { section = .overview }
        }
    }

    private var sectionPicker: some View {
        Picker(String(localized: "Section"), selection: $section) {
            ForEach(availableSections) { s in
                Text(s.title).tag(s)
            }
        }
        .pickerStyle(.segmented)
    }

    @ViewBuilder
    private var sectionContent: some View {
        switch section {
        case .overview:
            ServerOverviewSection(
                serverId: serverId,
                live: live,
                config: viewModel.config,
                groupName: groupName,
                capabilities: capabilities,
                isAdmin: isAdmin
            )
        case .metrics:
            MetricsContentView(serverId: serverId)
        case .traffic:
            ServerTrafficSection(serverId: serverId, config: viewModel.config)
        case .network:
            ServerNetworkSection(serverId: serverId, isAdmin: isAdmin)
        case .security:
            ServerSecuritySection(serverId: serverId)
        }
    }
}

/// Detail sections rendered as a segmented control.
enum DetailSection: String, Identifiable, CaseIterable {
    case overview
    case metrics
    case traffic
    case network
    case security

    var id: String { rawValue }

    var title: String {
        switch self {
        case .overview: String(localized: "Overview")
        case .metrics: String(localized: "Metrics")
        case .traffic: String(localized: "Traffic")
        case .network: String(localized: "Network")
        case .security: String(localized: "Security")
        }
    }
}
