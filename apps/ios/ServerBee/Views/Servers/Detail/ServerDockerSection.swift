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
                if let info = viewModel.info {
                    DockerInfoCard(info: info)
                }
                filterBar
                if filteredContainers.isEmpty {
                    emptyContainers
                } else {
                    ForEach(filteredContainers) { container in
                        Button { selected = container } label: {
                            DockerContainerRow(container: container, stats: viewModel.stats(for: container))
                        }
                        .buttonStyle(.plain)
                    }
                }
                resourcesCard
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

    private var filterBar: some View {
        Picker(String(localized: "Filter"), selection: $filter) {
            ForEach(ContainerFilter.allCases) { f in
                Text("\(f.label) (\(count(for: f)))").tag(f)
            }
        }
        .pickerStyle(.segmented)
    }

    private func count(for filter: ContainerFilter) -> Int {
        switch filter {
        case .all: viewModel.containers.count
        case .running: viewModel.containers.filter(\.isRunning).count
        case .stopped: viewModel.containers.filter { !$0.isRunning }.count
        }
    }

    private var emptyContainers: some View {
        ContentUnavailableView {
            Label(String(localized: "No containers"), systemImage: "shippingbox")
        } description: {
            Text(String(localized: "This host has no containers in this filter."))
        }
        .frame(minHeight: 220)
    }

    private var resourcesCard: some View {
        SectionCard(String(localized: "Resources"), systemImage: "square.stack.3d.up") {
            VStack(spacing: 0) {
                resourceRow(.events, title: String(localized: "Events"), systemImage: "list.bullet.rectangle")
                Divider()
                resourceRow(.networks, title: String(localized: "Networks"), systemImage: "network")
                Divider()
                resourceRow(.volumes, title: String(localized: "Volumes"), systemImage: "externaldrive")
            }
        }
    }

    private func resourceRow(_ res: DockerResource, title: String, systemImage: String) -> some View {
        Button { resource = res } label: {
            HStack(spacing: 10) {
                Image(systemName: systemImage).frame(width: 22).foregroundStyle(Color.brandAccent)
                Text(title).foregroundStyle(.primary)
                Spacer()
                Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
            }
            .padding(.vertical, 10)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
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

// MARK: - Info card

struct DockerInfoCard: View {
    let info: DockerSystemInfo

    private let columns = [GridItem(.flexible()), GridItem(.flexible())]

    var body: some View {
        SectionCard(String(localized: "Docker"), systemImage: "shippingbox.fill") {
            VStack(spacing: 12) {
                LazyVGrid(columns: columns, spacing: 12) {
                    stat(String(localized: "Running"), "\(info.containersRunning)", color: .serverOnline)
                    stat(String(localized: "Stopped"), "\(info.containersStopped)", color: .serverOffline)
                    stat(String(localized: "Paused"), "\(info.containersPaused)", color: .warningAmber)
                    stat(String(localized: "Images"), "\(info.images)")
                }
                Divider()
                DetailRow(label: String(localized: "Version"), value: info.dockerVersion)
                DetailRow(label: String(localized: "API"), value: info.apiVersion)
                DetailRow(label: String(localized: "Platform"), value: "\(info.os) · \(info.arch)")
                DetailRow(label: String(localized: "Memory"), value: Formatters.formatBytes(info.memoryTotal))
            }
        }
    }

    private func stat(_ label: String, _ value: String, color: Color = .primary) -> some View {
        VStack(spacing: 2) {
            Text(value).font(.title3.bold()).foregroundStyle(color)
            Text(label).font(.caption).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 8)
        .background(Color(.secondarySystemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}

// MARK: - Container row

struct DockerContainerRow: View {
    let container: DockerContainer
    let stats: DockerContainerStats?

    var body: some View {
        SectionCard {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text(container.displayName)
                        .font(.subheadline.bold())
                        .lineLimit(1)
                    Spacer()
                    DockerStatePill(state: container.state)
                }
                Text(container.image)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                if let stats, container.isRunning {
                    HStack(spacing: 14) {
                        Label(Formatters.formatPercentage(stats.cpuPercent), systemImage: "cpu")
                            .foregroundStyle(Formatters.cpuColor(for: stats.cpuPercent))
                        Label(Formatters.formatBytes(stats.memoryUsage), systemImage: "memorychip")
                            .foregroundStyle(Formatters.usageColor(for: stats.memoryPercent))
                        Label("↓\(Formatters.formatBytes(stats.networkRx))", systemImage: "arrow.down")
                            .foregroundStyle(.secondary)
                    }
                    .font(.caption)
                    .lineLimit(1)
                } else {
                    Text(container.status)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
        }
    }
}

/// Coloured pill for a container's state.
struct DockerStatePill: View {
    let state: String

    private var color: Color {
        switch state.lowercased() {
        case "running": .serverOnline
        case "paused": .warningAmber
        case "restarting", "created": .brandAccent
        default: .serverOffline
        }
    }

    var body: some View {
        Text(state.capitalized)
            .font(.caption2.weight(.semibold))
            .foregroundStyle(color)
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(color.opacity(0.14))
            .clipShape(Capsule())
    }
}
