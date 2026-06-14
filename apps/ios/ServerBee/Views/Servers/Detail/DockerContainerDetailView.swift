import SwiftUI

/// Container detail sheet: metadata, live stats, admin actions (start / stop /
/// restart / remove), and an entry into the live log viewer.
struct DockerContainerDetailView: View {
    let serverId: String
    let container: DockerContainer
    let stats: DockerContainerStats?
    let isAdmin: Bool
    let viewModel: DockerViewModel

    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss

    @State private var showStopConfirm = false
    @State private var showRestartConfirm = false
    @State private var showRemoveConfirm = false

    private var isPending: Bool { viewModel.pendingActions.contains(container.id) }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 16) {
                    metaCard
                    if let stats, container.isRunning {
                        statsCard(stats)
                    }
                    logsCard
                    if isAdmin {
                        actionsCard
                    }
                    if let error = viewModel.actionError {
                        Label(error, systemImage: "exclamationmark.triangle.fill")
                            .font(.caption)
                            .foregroundStyle(Color.serverOffline)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    if !container.ports.isEmpty {
                        portsCard
                    }
                    if !container.labels.isEmpty {
                        labelsCard
                    }
                }
                .padding()
            }
            .background(Color(.systemGroupedBackground))
            .navigationTitle(container.displayName)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(String(localized: "Done")) { dismiss() }
                }
            }
        }
    }

    // MARK: - Meta

    private var metaCard: some View {
        SectionCard {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    DockerStatePill(state: container.state)
                    Spacer()
                    Text(container.status).font(.caption).foregroundStyle(.secondary)
                }
                DetailRow(label: String(localized: "Image"), value: container.image, monospaced: true)
                DetailRow(label: String(localized: "Container ID"), value: String(container.id.prefix(12)), monospaced: true)
                DetailRow(label: String(localized: "Created"),
                          value: Date(timeIntervalSince1970: TimeInterval(container.created))
                            .formatted(date: .abbreviated, time: .shortened))
            }
        }
    }

    // MARK: - Stats

    private func statsCard(_ stats: DockerContainerStats) -> some View {
        SectionCard(String(localized: "Stats"), systemImage: "chart.bar") {
            VStack(spacing: 10) {
                statRow(String(localized: "CPU"), Formatters.formatPercentage(stats.cpuPercent),
                        color: Formatters.cpuColor(for: stats.cpuPercent))
                statRow(String(localized: "Memory"),
                        "\(Formatters.formatBytes(stats.memoryUsage)) / \(Formatters.formatBytes(stats.memoryLimit)) (\(Formatters.formatPercentage(stats.memoryPercent)))",
                        color: Formatters.usageColor(for: stats.memoryPercent))
                statRow(String(localized: "Network"),
                        "↓ \(Formatters.formatBytes(stats.networkRx))  ↑ \(Formatters.formatBytes(stats.networkTx))")
                statRow(String(localized: "Block I/O"),
                        "R \(Formatters.formatBytes(stats.blockRead))  W \(Formatters.formatBytes(stats.blockWrite))")
            }
        }
    }

    private func statRow(_ label: String, _ value: String, color: Color = .primary) -> some View {
        DetailRow(label: label, value: value, valueColor: color)
    }

    // MARK: - Logs

    private var logsCard: some View {
        SectionCard {
            if isAdmin {
                NavigationLink {
                    DockerLogsView(serverId: serverId, container: container)
                } label: {
                    HStack(spacing: 10) {
                        Image(systemName: "doc.text.magnifyingglass").frame(width: 22)
                        Text(String(localized: "View live logs"))
                        Spacer()
                        Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
                    }
                    .foregroundStyle(Color.brandAccent)
                    .contentShape(Rectangle())
                }
            } else {
                Label(String(localized: "Log streaming requires an admin account."), systemImage: "lock")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    // MARK: - Actions

    private var actionsCard: some View {
        SectionCard(String(localized: "Actions"), systemImage: "bolt") {
            VStack(spacing: 0) {
                if container.isRunning {
                    actionRow(String(localized: "Restart"), systemImage: "arrow.clockwise", tint: .brandAccent) {
                        showRestartConfirm = true
                    }
                    Divider()
                    actionRow(String(localized: "Stop"), systemImage: "stop.fill", tint: .warningAmber) {
                        showStopConfirm = true
                    }
                } else {
                    actionRow(String(localized: "Start"), systemImage: "play.fill", tint: .serverOnline) {
                        Task { await run(.start) }
                    }
                }
                Divider()
                actionRow(String(localized: "Remove"), systemImage: "trash", tint: .serverOffline) {
                    showRemoveConfirm = true
                }
            }
        }
        .confirmationDialog(String(localized: "Stop container?"), isPresented: $showStopConfirm, titleVisibility: .visible) {
            Button(String(localized: "Stop"), role: .destructive) { Task { await run(.stop(timeout: nil)) } }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(format: String(localized: "“%@” will be gracefully stopped."), container.displayName))
        }
        .confirmationDialog(String(localized: "Restart container?"), isPresented: $showRestartConfirm, titleVisibility: .visible) {
            Button(String(localized: "Restart")) { Task { await run(.restart(timeout: nil)) } }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(format: String(localized: "“%@” will be restarted."), container.displayName))
        }
        .confirmationDialog(String(localized: "Remove container?"), isPresented: $showRemoveConfirm, titleVisibility: .visible) {
            Button(String(localized: "Remove"), role: .destructive) { Task { await run(.remove(force: false)) } }
            Button(String(localized: "Force remove"), role: .destructive) { Task { await run(.remove(force: true)) } }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(format: String(localized: "“%@” will be removed. This cannot be undone. Use Force remove if it is still running."), container.displayName))
        }
    }

    private func actionRow(_ title: String, systemImage: String, tint: Color, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: systemImage).frame(width: 22)
                Text(title)
                Spacer()
                if isPending {
                    ProgressView()
                }
            }
            .foregroundStyle(tint)
            .contentShape(Rectangle())
            .padding(.vertical, 10)
        }
        .buttonStyle(.plain)
        .disabled(isPending)
    }

    private func run(_ action: DockerAction) async {
        let ok = await viewModel.perform(action, on: container.id, serverId: serverId, apiClient: apiClient)
        if ok, case .remove = action {
            dismiss()
        }
    }

    // MARK: - Ports / labels

    private var portsCard: some View {
        SectionCard(String(localized: "Ports"), systemImage: "point.3.connected.trianglepath.dotted") {
            FlowChips(items: container.ports.map(\.display)) { port in
                Chip(text: port, color: .networkColor)
            }
        }
    }

    private var labelsCard: some View {
        SectionCard(String(localized: "Labels"), systemImage: "tag") {
            VStack(spacing: 6) {
                ForEach(container.labels.sorted(by: { $0.key < $1.key }), id: \.key) { key, value in
                    DetailRow(label: key, value: value, monospaced: true)
                }
            }
        }
    }
}
