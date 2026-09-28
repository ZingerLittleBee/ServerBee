import SwiftUI

/// The "Docker" tab of the server detail screen: system info, a filterable
/// container list with live stats, admin container actions, log streaming, and
/// on-demand events / networks / volumes.
///
/// Docker reads need the agent online + the "docker" feature; when unavailable
/// the section shows a friendly explanation instead of empty data.
struct ServerDockerSection: View {
    let serverId: String
    let isAdmin: Bool

    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = DockerViewModel()
    @State private var filter: ContainerFilter = .all
    @State private var selected: DockerContainer?
    @State private var resource: DockerResource?

    enum ContainerFilter: String, CaseIterable, Identifiable {
        case all, running, stopped
        var id: String { rawValue }
        var label: String {
            switch self {
            case .all: String(localized: "All")
            case .running: String(localized: "Running")
            case .stopped: String(localized: "Stopped")
            }
        }
    }

    enum DockerResource: String, Identifiable {
        case events, networks, volumes
        var id: String { rawValue }
    }

    var body: some View {
        Group {
            if viewModel.isLoading && !viewModel.hasLoaded {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let message = viewModel.unavailableMessage {
                unavailableView(message)
            } else {
                content
            }
        }
        .background(Color(.systemGroupedBackground))
        .refreshable { await viewModel.refresh(serverId: serverId, apiClient: apiClient) }
        .task {
            #if DEBUG
            if let token = UITestSupport.autoPresent, token.hasPrefix("docker") {
                DockerSampleData.populate(viewModel)
                if token == "docker-detail" { selected = viewModel.containers.first }
                return
            }
            #endif
            if !viewModel.hasLoaded {
                await viewModel.load(serverId: serverId, apiClient: apiClient)
            }
        }
        .sheet(item: $selected) { container in
            DockerContainerDetailView(
                serverId: serverId,
                container: container,
                stats: viewModel.stats(for: container),
                isAdmin: isAdmin,
                viewModel: viewModel
            )
        }
        .sheet(item: $resource) { res in
            DockerResourceSheet(serverId: serverId, resource: res, viewModel: viewModel)
        }
    }

    private var content: some View {
        ScrollView {
            VStack(spacing: 16) {
                DockerInfoCard(info: viewModel.info) { resource = $0 }
                filterBar
                if filteredContainers.isEmpty {
                    emptyContainers
                } else {
                    containerList
                }
            }
            .padding()
        }
    }

    private var filteredContainers: [DockerContainer] {
        switch filter {
        case .all: viewModel.containers
        case .running: viewModel.containers.filter(\.isRunning)
        case .stopped: viewModel.containers.filter { !$0.isRunning }
        }
    }

    // MARK: - Filter

    private var filterBar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(ContainerFilter.allCases) { option in
                    filterPill(option)
                }
            }
        }
        .scrollClipDisabled()
        .animation(.snappy, value: filter)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(String(localized: "Filter"))
    }

    private func filterPill(_ option: ContainerFilter) -> some View {
        let isSelected = filter == option
        return Button {
            filter = option
        } label: {
            Text(verbatim: "\(option.label) (\(count(for: option)))")
                .font(.subheadline.weight(.semibold))
                .monospacedDigit()
                .lineLimit(1)
                .foregroundStyle(isSelected ? Color.white : Color.primary)
                .padding(.horizontal, 16)
                .padding(.vertical, 6)
                .background(isSelected ? Color.accentColor : Color(.tertiarySystemFill), in: Capsule())
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }

    private func count(for filter: ContainerFilter) -> Int {
        switch filter {
        case .all: viewModel.containers.count
        case .running: viewModel.containers.filter(\.isRunning).count
        case .stopped: viewModel.containers.filter { !$0.isRunning }.count
        }
    }

    // MARK: - Containers

    private var containerList: some View {
        let containers = filteredContainers
        return VStack(spacing: 0) {
            ForEach(Array(containers.enumerated()), id: \.element.id) { index, container in
                Button { selected = container } label: {
                    DockerContainerRow(
                        container: container,
                        stats: viewModel.stats(for: container),
                        showsSeparator: index < containers.count - 1
                    )
                }
                .buttonStyle(GroupedRowButtonStyle())
            }
        }
        .background(Color(.secondarySystemGroupedBackground))
        .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
    }

    private var emptyContainers: some View {
        ContentUnavailableView {
            Label(String(localized: "No containers"), systemImage: "shippingbox")
        } description: {
            Text(String(localized: "This host has no containers in this filter."))
        }
        .frame(minHeight: 220)
    }

    private func unavailableView(_ message: String) -> some View {
        ContentUnavailableView {
            Label(String(localized: "Docker unavailable"), systemImage: "shippingbox")
        } description: {
            Text(message)
        } actions: {
            Button(String(localized: "Try again")) {
                Task { await viewModel.load(serverId: serverId, apiClient: apiClient) }
            }
            .buttonStyle(.borderedProminent)
        }
    }
}

// MARK: - Resources card

/// "Resources" card: engine version, container / image counts, host platform,
/// and entry points into the on-demand events / networks / volumes sheets.
/// Info is best-effort, so the entry points stay reachable when it is missing.
struct DockerInfoCard: View {
    let info: DockerSystemInfo?
    let onOpen: (ServerDockerSection.DockerResource) -> Void

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            header
            if let info {
                statsGrid(info)
                Text(verbatim: footnote(for: info))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            resourceButtons
        }
        .cardSurface()
    }

    private var header: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                title
                Spacer(minLength: 8)
                versionText.lineLimit(1)
            }
            // Stacked fallback lets the version wrap instead of truncating.
            VStack(alignment: .leading, spacing: 2) {
                title
                versionText
            }
        }
    }

    private var title: some View {
        Text(String(localized: "Resources"))
            .font(.subheadline.weight(.semibold))
            .accessibilityAddTraits(.isHeader)
    }

    @ViewBuilder
    private var versionText: some View {
        if let info {
            Text(verbatim: "Docker \(info.dockerVersion) · API \(info.apiVersion)")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .monospacedDigit()
        }
    }

    private func statsGrid(_ info: DockerSystemInfo) -> some View {
        let columns = Array(
            repeating: GridItem(.flexible(), spacing: 8, alignment: .leading),
            count: dynamicTypeSize.isAccessibilitySize ? 2 : 4
        )
        return LazyVGrid(columns: columns, alignment: .leading, spacing: 12) {
            stat(String(localized: "Running"), info.containersRunning,
                 color: info.containersRunning > 0 ? .serverOnline : .secondary)
            stat(String(localized: "Stopped"), info.containersStopped, color: .secondary)
            stat(String(localized: "Paused"), info.containersPaused,
                 color: info.containersPaused > 0 ? .warningAmber : .secondary)
            stat(String(localized: "Images"), info.images, color: .primary)
        }
    }

    private func stat(_ label: String, _ value: Int64, color: Color) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            StatValue(value: "\(value)", color: color)
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(label))
        .accessibilityValue(Text(verbatim: "\(value)"))
    }

    /// "linux/x86_64 · 8 GB memory".
    private func footnote(for info: DockerSystemInfo) -> String {
        let platform = [info.os, info.arch].filter { !$0.isEmpty }.joined(separator: "/")
        let memory = String(localized: "\(dockerBytes(info.memoryTotal)) memory")
        return [platform, memory].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    private var resourceButtons: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 8) { resourceButtonSet }
            VStack(spacing: 8) { resourceButtonSet }
        }
    }

    @ViewBuilder
    private var resourceButtonSet: some View {
        resourceButton(.events, title: String(localized: "Events"), systemImage: "list.bullet.rectangle")
        resourceButton(.networks, title: String(localized: "Networks"), systemImage: "network")
        resourceButton(.volumes, title: String(localized: "Volumes"), systemImage: "externaldrive")
    }

    private func resourceButton(
        _ res: ServerDockerSection.DockerResource,
        title: String,
        systemImage: String
    ) -> some View {
        Button { onOpen(res) } label: {
            Label(title, systemImage: systemImage)
                .font(.footnote.weight(.semibold))
                .lineLimit(1)
                // With the bordered insets this yields a 44pt tap target.
                .frame(maxWidth: .infinity, minHeight: 30)
        }
        .buttonStyle(.bordered)
        .buttonBorderShape(.roundedRectangle(radius: 10))
    }
}

// MARK: - Container row

/// One container in the grouped list: icon tile, name + state badge, image,
/// and live CPU / memory / network for running containers (status otherwise).
struct DockerContainerRow: View {
    let container: DockerContainer
    let stats: DockerContainerStats?
    var showsSeparator = false

    @ScaledMetric(relativeTo: .body) private var tileSize: CGFloat = 30
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    /// At accessibility sizes the decorative tile is dropped, the badge moves
    /// under the name and text may wrap, so long names stay readable.
    private var isLarge: Bool { dynamicTypeSize.isAccessibilitySize }

    private var titleLayout: AnyLayout {
        isLarge
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 3))
            : AnyLayout(HStackLayout(spacing: 6))
    }

    var body: some View {
        HStack(spacing: 12) {
            if !isLarge {
                Image(systemName: "shippingbox")
                    .font(.system(size: tileSize * 0.58))
                    .foregroundStyle(.secondary)
                    .frame(width: tileSize, height: tileSize)
                    .background(
                        Color(.tertiarySystemFill),
                        in: RoundedRectangle(cornerRadius: tileSize * 0.27, style: .continuous)
                    )
                    .accessibilityHidden(true)
            }
            VStack(alignment: .leading, spacing: 3) {
                titleLayout {
                    Text(container.displayName)
                        .font(.callout.weight(.semibold))
                        .foregroundStyle(.primary)
                    DockerStatePill(state: container.state)
                        .fixedSize()
                }
                Text(container.image)
                    .font(.footnote.monospaced())
                    .foregroundStyle(.secondary)
                detailLine
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            .lineLimit(isLarge ? 3 : 1)
            Spacer(minLength: 8)
            Image(systemName: "chevron.right")
                .font(.footnote.weight(.semibold))
                .foregroundStyle(.tertiary)
                .accessibilityHidden(true)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .overlay(alignment: .bottom) {
            if showsSeparator {
                Divider().padding(.leading, isLarge ? 16 : 16 + tileSize + 12)
            }
        }
    }

    private var detailLine: Text {
        if let stats, container.isRunning {
            return Text(Self.statsSummary(stats))
        }
        return Text(verbatim: container.status)
    }

    /// "CPU 2.4% · 50.0 MB · ↓ 1.0 MB"; CPU / memory switch to the warning or
    /// error colour when usage is elevated.
    private static func statsSummary(_ stats: DockerContainerStats) -> AttributedString {
        var cpu = AttributedString("\(String(localized: "CPU")) \(Formatters.formatPercentage(stats.cpuPercent))")
        cpu.foregroundColor = loadColor(for: stats.cpuPercent)
        var memory = AttributedString(dockerBytes(stats.memoryUsage))
        memory.foregroundColor = loadColor(for: stats.memoryPercent)
        let separator = AttributedString(" · ")
        let received = AttributedString("↓\u{00A0}\(dockerBytes(stats.networkRx))")
        return cpu + separator + memory + separator + received
    }

    /// `nil` keeps the row's secondary colour for normal load.
    private static func loadColor(for percent: Double) -> Color? {
        switch percent {
        case ..<50: nil
        case ..<80: .warningAmber
        default: .serverOffline
        }
    }
}

/// Byte count with a non-breaking space ("8\u{00A0}GB") so the number and unit
/// never wrap onto separate lines.
private func dockerBytes(_ bytes: Int64) -> String {
    Formatters.formatBytes(bytes).replacingOccurrences(of: " ", with: "\u{00A0}")
}

/// Row highlight for buttons laid out as grouped-list rows.
private struct GroupedRowButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(configuration.isPressed ? Color(.systemGray4) : Color.clear)
    }
}

/// Tinted badge for a container's state.
struct DockerStatePill: View {
    let state: String

    /// Running is healthy, transitional states warn, `dead` is an error, and
    /// everything else (exited, created, …) is neutral.
    static func color(for state: String) -> Color {
        switch state.lowercased() {
        case "running": .serverOnline
        case "paused", "restarting", "removing": .warningAmber
        case "dead": .serverOffline
        default: .secondary
        }
    }

    /// Localized label for Docker's known states; unknown states show as-is.
    static func label(for state: String) -> String {
        switch state.lowercased() {
        case "running": String(localized: "Running")
        case "paused": String(localized: "Paused")
        case "restarting": String(localized: "Restarting")
        case "created": String(localized: "Created")
        case "exited": String(localized: "Exited")
        case "removing": String(localized: "Removing")
        case "dead": String(localized: "Dead")
        default: state.capitalized
        }
    }

    var body: some View {
        StatusBadge(text: Self.label(for: state), color: Self.color(for: state))
    }
}
