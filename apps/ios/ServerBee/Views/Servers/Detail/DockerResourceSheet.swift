import SwiftUI

/// Sheet that lists one of the secondary Docker resources: events, networks, or
/// volumes. Each loads on demand.
struct DockerResourceSheet: View {
    let serverId: String
    let resource: ServerDockerSection.DockerResource
    let viewModel: DockerViewModel

    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss
    @State private var isLoading = true

    var body: some View {
        NavigationStack {
            Group {
                if isLoading {
                    ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    list
                }
            }
            .background(Color(.systemGroupedBackground))
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(String(localized: "Done")) { dismiss() }
                }
            }
            .task { await loadIfNeeded() }
        }
    }

    private var title: String {
        switch resource {
        case .events: String(localized: "Events")
        case .networks: String(localized: "Networks")
        case .volumes: String(localized: "Volumes")
        }
    }

    @ViewBuilder
    private var list: some View {
        switch resource {
        case .events: eventsList
        case .networks: networksList
        case .volumes: volumesList
        }
    }

    private var eventsList: some View {
        Group {
            if viewModel.events.isEmpty {
                emptyState(String(localized: "No recent events"))
            } else {
                ScrollView {
                    VStack(spacing: 10) {
                        ForEach(viewModel.events) { event in
                            SectionCard {
                                VStack(alignment: .leading, spacing: 6) {
                                    HStack {
                                        Chip(text: event.eventType, color: .brandAccent)
                                        Text(event.action).font(.subheadline.bold())
                                        Spacer()
                                        Text(Date(timeIntervalSince1970: TimeInterval(event.timestamp)),
                                             format: .relative(presentation: .named))
                                            .font(.caption2).foregroundStyle(.secondary)
                                    }
                                    if let name = event.actorName ?? (event.actorId.isEmpty ? nil : String(event.actorId.prefix(12))) {
                                        Text(name).font(.caption.monospaced()).foregroundStyle(.secondary)
                                    }
                                }
                            }
                        }
                    }
                    .padding()
                }
            }
        }
    }

    private var networksList: some View {
        Group {
            if viewModel.networks.isEmpty {
                emptyState(String(localized: "No networks"))
            } else {
                ScrollView {
                    VStack(spacing: 10) {
                        ForEach(viewModel.networks) { network in
                            SectionCard {
                                VStack(alignment: .leading, spacing: 6) {
                                    HStack {
                                        Text(network.name).font(.subheadline.bold())
                                        Spacer()
                                        Chip(text: network.driver, color: .networkColor)
                                    }
                                    DetailRow(label: String(localized: "Scope"), value: network.scope)
                                    DetailRow(label: String(localized: "Containers"), value: "\(network.containers.count)")
                                }
                            }
                        }
                    }
                    .padding()
                }
            }
        }
    }

    private var volumesList: some View {
        Group {
            if viewModel.volumes.isEmpty {
                emptyState(String(localized: "No volumes"))
            } else {
                ScrollView {
                    VStack(spacing: 10) {
                        ForEach(viewModel.volumes) { volume in
                            SectionCard {
                                VStack(alignment: .leading, spacing: 6) {
                                    HStack {
                                        Text(volume.name).font(.subheadline.bold()).lineLimit(1)
                                        Spacer()
                                        Chip(text: volume.driver, color: .diskColor)
                                    }
                                    DetailRow(label: String(localized: "Mount"), value: volume.mountpoint, monospaced: true)
                                    if let created = volume.createdAt {
                                        DetailRow(label: String(localized: "Created"), value: created)
                                    }
                                }
                            }
                        }
                    }
                    .padding()
                }
            }
        }
    }

    private func emptyState(_ text: String) -> some View {
        ContentUnavailableView {
            Label(text, systemImage: "tray")
        }
    }

    private func loadIfNeeded() async {
        isLoading = true
        switch resource {
        case .events: await viewModel.loadEvents(serverId: serverId, apiClient: apiClient)
        case .networks: await viewModel.loadNetworks(serverId: serverId, apiClient: apiClient)
        case .volumes: await viewModel.loadVolumes(serverId: serverId, apiClient: apiClient)
        }
        isLoading = false
    }
}

#if DEBUG
/// Sample Docker data for headless visual verification (the shared demo has no
/// Docker-enabled server). Populated only via the `docker-sample` launch hook.
enum DockerSampleData {
    @MainActor
    static func populate(_ viewModel: DockerViewModel) {
        viewModel.info = DockerSystemInfo(
            dockerVersion: "27.1.1", apiVersion: "1.46", os: "linux", arch: "x86_64",
            containersRunning: 3, containersPaused: 0, containersStopped: 1, images: 12,
            memoryTotal: 8_589_934_592
        )
        viewModel.containers = [
            DockerContainer(id: "a1b2c3d4e5f6", name: "/web", image: "nginx:alpine",
                            state: "running", status: "Up 3 hours", created: 1_748_000_000,
                            ports: [DockerPort(privatePort: 80, publicPort: 8080, portType: "tcp", ip: "0.0.0.0")],
                            labels: ["com.docker.compose.project": "serverbee"]),
            DockerContainer(id: "b2c3d4e5f6a1", name: "/db", image: "postgres:16",
                            state: "running", status: "Up 3 hours (healthy)", created: 1_748_000_500,
                            ports: [DockerPort(privatePort: 5432, publicPort: nil, portType: "tcp", ip: nil)],
                            labels: [:]),
            DockerContainer(id: "c3d4e5f6a1b2", name: "/cache", image: "redis:7-alpine",
                            state: "exited", status: "Exited (0) 2 hours ago", created: 1_747_900_000,
                            ports: [], labels: [:])
        ]
        viewModel.statsById = [
            "a1b2c3d4e5f6": DockerContainerStats(id: "a1b2c3d4e5f6", name: "web", cpuPercent: 2.4,
                memoryUsage: 52_428_800, memoryLimit: 536_870_912, memoryPercent: 9.8,
                networkRx: 1_048_576, networkTx: 524_288, blockRead: 0, blockWrite: 10_240),
            "b2c3d4e5f6a1": DockerContainerStats(id: "b2c3d4e5f6a1", name: "db", cpuPercent: 18.7,
                memoryUsage: 268_435_456, memoryLimit: 1_073_741_824, memoryPercent: 25.0,
                networkRx: 4_194_304, networkTx: 2_097_152, blockRead: 1_048_576, blockWrite: 5_242_880)
        ]
        viewModel.events = [
            DockerEventInfo(timestamp: 1_748_010_000, eventType: "container", action: "start",
                            actorId: "a1b2c3d4e5f6", actorName: "web", attributes: [:]),
            DockerEventInfo(timestamp: 1_748_009_000, eventType: "container", action: "die",
                            actorId: "c3d4e5f6a1b2", actorName: "cache", attributes: [:])
        ]
        viewModel.networks = [
            DockerNetwork(id: "n1", name: "bridge", driver: "bridge", scope: "local", containers: ["a": "web"]),
            DockerNetwork(id: "n2", name: "serverbee_default", driver: "bridge", scope: "local", containers: [:])
        ]
        viewModel.volumes = [
            DockerVolume(name: "pgdata", driver: "local", mountpoint: "/var/lib/docker/volumes/pgdata/_data",
                         createdAt: "2026-05-20T10:00:00Z", labels: [:])
        ]
        viewModel.hasLoaded = true
    }
}
#endif
