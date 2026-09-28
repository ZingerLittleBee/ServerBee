import SwiftUI

/// Native detail screen for a single server.
///
/// Live runtime state (status, metrics) comes from the WebSocket-backed
/// `ServersViewModel` so the screen updates in real time; static configuration
/// (capabilities, billing, kernel, agent version, enrollment) comes from a REST
/// fetch. Sections are gated on the server's *effective* capabilities and the
/// caller's role.
///
/// The server name is the native large navigation title (it collapses as the
/// section content scrolls). Status chips and the section picker sit in a top
/// safe-area inset so they stay reachable on every section, each of which owns
/// its own scroll view.
struct ServerDetailView: View {
    let serverId: String

    @Environment(ServersViewModel.self) private var serversViewModel
    @Environment(\.apiClient) private var apiClient
    @Environment(AuthManager.self) private var authManager
    @State private var viewModel = ServerDetailViewModel()
    @State private var section: DetailSection = .overview
    @State private var showEdit = false
    /// Section content is created one tick after the screen appears; see `body`.
    @State private var isContentReady = false

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
    /// Used for *effective* checks (what the agent can do right now). A live
    /// entry built only from WS frames (e.g. opened via deep link before the
    /// list's REST merge) carries no capability bits, so it doesn't count.
    private var capabilities: CapabilitySet {
        if let liveCaps = live?.capabilitySet,
           liveCaps.configured != nil || liveCaps.agentLocal != nil || liveCaps.effective != nil {
            return liveCaps
        }
        return viewModel.config?.capabilitySet ?? CapabilitySet()
    }

    /// Capability set used for *section visibility*. Prefers the REST config,
    /// which carries the admin-configured mask (the live WS frame may omit it),
    /// so historical data (security / IP quality / network / traffic) stays
    /// viewable even while the agent is offline.
    private var sectionCaps: CapabilitySet {
        viewModel.config?.capabilitySet ?? live?.capabilitySet ?? CapabilitySet()
    }

    private var displayName: String {
        live?.name ?? viewModel.config?.name ?? String(localized: "Server")
    }

    private var groupName: String? {
        if let live { return serversViewModel.resolvedGroupName(for: live) }
        if let gid = viewModel.config?.groupId { return serversViewModel.groupsByID[gid] }
        return nil
    }

    /// Which sections to show, in picker order. Traffic (usage / cost / uptime)
    /// is always available since uptime applies to every enrolled server.
    /// Network, Security and IP quality appear when the server is *configured*
    /// for them (so their historical data is viewable even while the agent is
    /// offline).
    private var availableSections: [DetailSection] {
        var result: [DetailSection] = [.overview, .metrics]
        if sectionCaps.isConfigured(.pingICMP) || sectionCaps.isConfigured(.pingTCP) || sectionCaps.isConfigured(.pingHTTP) {
            result.append(.network)
        }
        result.append(.traffic)
        if sectionCaps.isConfigured(.securityEvents) { result.append(.security) }
        if sectionCaps.isConfigured(.ipQuality) { result.append(.ipQuality) }
        if sectionCaps.isConfigured(.docker) { result.append(.docker) }
        #if DEBUG
        // Visual-verification hook: the shared demo has no Docker-enabled server,
        // so allow forcing the Docker section into view for screenshots.
        if UITestSupport.autoPresent?.hasPrefix("docker") == true, !result.contains(.docker) {
            result.append(.docker)
        }
        #endif
        return result
    }

    var body: some View {
        // Creating the section's scroll view only after the push lets it pick
        // up the pinned header inset before its first offset is set; otherwise
        // iOS 26 starts it scrolled by the header height, collapsing the large
        // title on arrival.
        Group {
            if isContentReady {
                sectionContent
            } else {
                Color.clear
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .pinnedTopBar { header }
        .onAppear { isContentReady = true }
        .background(Color(.systemGroupedBackground))
        .navigationTitle(displayName)
        .navigationBarTitleDisplayMode(.large)
        .toolbar {
            if isAdmin, viewModel.config != nil {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { showEdit = true } label: {
                        Label(String(localized: "Edit server"), systemImage: "pencil")
                    }
                }
            }
        }
        .sheet(isPresented: $showEdit) {
            if let config = viewModel.config {
                EditServerSheet(serverId: serverId, config: config, onSaved: reloadConfig)
            }
        }
        .task {
            await viewModel.fetchConfig(serverId: serverId, apiClient: apiClient)
            #if DEBUG
            if let raw = UITestSupport.detailSection,
               let target = DetailSection(rawValue: raw),
               availableSections.contains(target) {
                section = target
            }
            debugPresentEditIfReady()
            #endif
        }
        .onChange(of: availableSections) { _, sections in
            // If the active section disappears (caps changed), fall back.
            if !sections.contains(section) { section = .overview }
        }
        #if DEBUG
        // Visual-verification hook: auto-open the edit form (a toolbar button
        // the cliclick harness can't reliably activate). `config` loads
        // async, so also fire when it becomes available.
        .onChange(of: viewModel.config != nil) { _, _ in debugPresentEditIfReady() }
        #endif
    }

    #if DEBUG
    private func debugPresentEditIfReady() {
        if UITestSupport.autoPresent == "edit-server", viewModel.config != nil { showEdit = true }
    }
    #endif

    private func reloadConfig() {
        Task { await viewModel.fetchConfig(serverId: serverId, apiClient: apiClient) }
    }

    @ViewBuilder
    private var sectionContent: some View {
        switch section {
        case .overview:
            ServerOverviewSection(
                serverId: serverId,
                live: live,
                config: viewModel.config,
                capabilities: capabilities,
                isAdmin: isAdmin,
                onReloadConfig: reloadConfig
            )
        case .metrics:
            MetricsContentView(serverId: serverId)
        case .traffic:
            ServerTrafficSection(serverId: serverId, config: viewModel.config)
        case .network:
            ServerNetworkSection(serverId: serverId, isAdmin: isAdmin)
        case .security:
            ServerSecuritySection(serverId: serverId)
        case .ipQuality:
            ServerIpQualitySection(serverId: serverId, isAdmin: isAdmin, isOnline: isOnline)
        case .docker:
            ServerDockerSection(serverId: serverId, isAdmin: isAdmin)
        }
    }
}

// MARK: - Header

private extension ServerDetailView {
    /// Pinned header: one line of status chips plus the section picker. The
    /// chips scroll horizontally instead of wrapping so the pinned height stays
    /// constant as live / REST data arrives (a growing inset would shift the
    /// section's scroll offset and collapse the large title).
    var header: some View {
        VStack(alignment: .leading, spacing: 10) {
            ScrollView(.horizontal) {
                HStack(spacing: 6) {
                    statusChip
                    if let os = config?.os ?? live?.os, !os.isEmpty {
                        Chip(text: os, color: .primary)
                    }
                    if let spec = specText {
                        Chip(text: spec, color: .primary)
                    }
                    if let groupName {
                        Chip(text: groupName, color: .primary)
                    }
                    ForEach((live?.tags ?? []).prefix(4), id: \.self) { tag in
                        Chip(text: tag, systemImage: "tag", color: .primary)
                    }
                }
                .padding(.horizontal, 16)
            }
            .scrollIndicators(.hidden)
            if availableSections.count > 1 {
                DetailSectionPicker(sections: availableSections, selection: $section)
                    .padding(.horizontal, 16)
            }
        }
        .padding(.top, 2)
        .padding(.bottom, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    var config: ServerConfig? { viewModel.config }

    var isOnline: Bool { live?.isOnline ?? false }

    var statusChip: some View {
        let text = statusText
        return DetailStatusChip(text: text, color: isOnline ? .serverOnline : .serverOffline)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(Text(String(localized: "Status")))
            .accessibilityValue(Text(text))
    }

    /// "Online · 42d 3h" while connected, "Offline · 5 min. ago" once the agent
    /// dropped (falls back to the bare state when the timestamp is unknown).
    var statusText: String {
        if isOnline {
            guard let uptime = live?.uptime else { return String(localized: "Online") }
            return String(localized: "Online · \(Formatters.formatUptime(uptime))")
        }
        guard let last = live?.lastActiveAt else { return String(localized: "Offline") }
        return String(localized: "Offline · \(Formatters.formatRelativeTime(last))")
    }

    /// "4 vCPU · 8 GB" from whatever core count / memory total is known.
    var specText: String? {
        var parts: [String] = []
        if let cores = config?.cpuCores ?? live?.cpuCores, cores > 0 {
            parts.append(String(localized: "\(cores) vCPU"))
        }
        if let memory = live?.memoryTotal ?? config?.memTotal, memory > 0 {
            parts.append(Formatters.formatBytes(memory))
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }
}

/// `Chip`-shaped status capsule whose text is deepened in light mode: the
/// bright status greens / reds are too faint as caption text on their own tint.
private struct DetailStatusChip: View {
    let text: String
    let color: Color

    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        Text(text)
            .font(.caption.weight(.semibold))
            .monospacedDigit()
            .foregroundStyle(color)
            .brightness(colorScheme == .dark ? 0 : -0.18)
            .padding(.horizontal, 9)
            .padding(.vertical, 3)
            .background(color.opacity(0.14), in: Capsule())
    }
}

private extension View {
    /// Pins `bar` below the navigation bar while each section's own scroll view
    /// keeps driving the large-title collapse. The bar is opaque, and the
    /// navigation bar takes the same grouped background once content scrolls
    /// under it, so the two read as one header and scrolled content never
    /// shows between them.
    func pinnedTopBar<Bar: View>(@ViewBuilder _ bar: () -> Bar) -> some View {
        safeAreaInset(edge: .top, spacing: 0) {
            // Kept out of the top safe area: on iOS 26 the large title is drawn
            // in the scroll layer, and a full-bleed background would cover it.
            bar().background(Color(.systemGroupedBackground), ignoresSafeAreaEdges: [])
        }
        .toolbarBackground(Color(.systemGroupedBackground), for: .navigationBar)
    }
}

// MARK: - Section picker

/// Segmented section selector styled like the native segmented control.
///
/// Up to five sections render inline. Beyond that the first four stay inline
/// and the rest move into a trailing "More" menu segment, which shows the
/// active overflow section's title (plus a chevron) while it is selected.
struct DetailSectionPicker: View {
    let sections: [DetailSection]
    @Binding var selection: DetailSection

    @Namespace private var namespace
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private static let maxInline = 5

    /// Raised segment fill: white on the light track, a lifted grey on dark.
    private static let selectedFill = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark ? .systemGray2 : .systemBackground
    })

    private var inlineSections: [DetailSection] {
        sections.count <= Self.maxInline ? sections : Array(sections.prefix(Self.maxInline - 1))
    }

    private var overflowSections: [DetailSection] {
        sections.count <= Self.maxInline ? [] : Array(sections.dropFirst(Self.maxInline - 1))
    }

    var body: some View {
        HStack(spacing: 0) {
            ForEach(inlineSections) { item in
                let isSelected = item == selection
                Button { select(item) } label: {
                    segment(isSelected: isSelected) { Text(item.title) }
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text(item.menuTitle))
                .accessibilityAddTraits(isSelected ? .isSelected : [])
            }
            if !overflowSections.isEmpty {
                moreMenu
            }
        }
        .padding(2)
        .background(Color(.tertiarySystemFill), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        // Scoped to the picker: animating the selection change itself would
        // also crossfade the section content, and on iOS 26 the large title
        // fades out with the outgoing scroll view until the new one mounts.
        .animation(reduceMotion ? nil : .snappy(duration: 0.25), value: selection)
        .accessibilityElement(children: .contain)
    }

    private var moreMenu: some View {
        let active = overflowSections.first { $0 == selection }
        return Menu {
            ForEach(overflowSections) { item in
                Toggle(isOn: Binding(
                    get: { selection == item },
                    set: { isOn in if isOn { select(item) } }
                )) {
                    Label(item.menuTitle, systemImage: item.systemImage)
                }
            }
        } label: {
            segment(isSelected: active != nil) {
                HStack(spacing: 3) {
                    Text(active?.title ?? String(localized: "More"))
                    if active != nil {
                        Image(systemName: "chevron.down")
                            .font(.caption2.weight(.bold))
                            .accessibilityHidden(true)
                    }
                }
            }
        }
        .menuStyle(.button)
        .menuIndicator(.hidden)
        .buttonStyle(.plain)
        .accessibilityLabel(Text(active?.menuTitle ?? String(localized: "More")))
        .accessibilityAddTraits(active != nil ? .isSelected : [])
    }

    private func segment<Content: View>(isSelected: Bool, @ViewBuilder content: () -> Content) -> some View {
        content()
            .font(.footnote.weight(isSelected ? .semibold : .regular))
            .foregroundStyle(.primary)
            .lineLimit(1)
            .minimumScaleFactor(0.75)
            .padding(.vertical, 6)
            .padding(.horizontal, 4)
            .frame(maxWidth: .infinity)
            .background {
                if isSelected {
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(Self.selectedFill)
                        .shadow(color: .black.opacity(0.1), radius: 1.5, y: 1)
                        .matchedGeometryEffect(id: "selected-segment", in: namespace)
                }
            }
            .contentShape(Rectangle())
    }

    private func select(_ item: DetailSection) {
        guard item != selection else { return }
        selection = item
    }
}

/// Detail sections rendered by `DetailSectionPicker`.
enum DetailSection: String, Identifiable, CaseIterable {
    case overview
    case metrics
    case traffic
    case network
    case security
    case ipQuality
    case docker

    var id: String { rawValue }

    /// Short segment title.
    var title: String {
        switch self {
        case .overview: String(localized: "Overview")
        case .metrics: String(localized: "Metrics")
        case .traffic: String(localized: "Traffic")
        case .network: String(localized: "Network")
        case .security: String(localized: "Security")
        case .ipQuality: String(localized: "IP")
        case .docker: String(localized: "Docker")
        }
    }

    /// Full title for the overflow menu and VoiceOver, where space allows.
    var menuTitle: String {
        switch self {
        case .ipQuality: String(localized: "IP quality")
        default: title
        }
    }

    var systemImage: String {
        switch self {
        case .overview: "square.grid.2x2"
        case .metrics: "chart.xyaxis.line"
        case .traffic: "arrow.up.arrow.down"
        case .network: "dot.radiowaves.left.and.right"
        case .security: "shield"
        case .ipQuality: "checkmark.seal"
        case .docker: "shippingbox"
        }
    }
}
