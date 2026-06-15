import SwiftUI

/// Live container log viewer backed by the docker-logs WebSocket (admin-only).
/// Streams `stdout`/`stderr` lines, auto-scrolls while following, and lets the
/// user pause auto-scroll and clear the buffer.
struct DockerLogsView: View {
    let serverId: String
    let container: DockerContainer

    @Environment(AuthManager.self) private var authManager
    @State private var viewModel: DockerLogsViewModel
    @State private var follow = true

    init(serverId: String, container: DockerContainer) {
        self.serverId = serverId
        self.container = container
        _viewModel = State(initialValue: DockerLogsViewModel(containerId: container.id, tail: 200, follow: true))
    }

    var body: some View {
        VStack(spacing: 0) {
            statusBar
            Divider()
            logScroll
        }
        .navigationTitle(String(localized: "Logs"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    viewModel.entries.removeAll()
                } label: {
                    Label(String(localized: "Clear"), systemImage: "trash")
                }
            }
        }
        .task {
            let token = (try? await authManager.refreshAccessToken()) ?? authManager.getAccessToken()
            guard let token, let serverUrl = authManager.serverUrl else {
                viewModel.errorMessage = String(localized: "Not signed in.")
                return
            }
            viewModel.start(serverUrl: serverUrl, accessToken: token, serverId: serverId)
        }
        .onDisappear { viewModel.stop() }
    }

    private var statusBar: some View {
        HStack(spacing: 10) {
            Circle()
                .fill(viewModel.isConnected ? Color.serverOnline : Color.serverOffline)
                .frame(width: 8, height: 8)
            Text(viewModel.isConnected ? String(localized: "Streaming") : String(localized: "Connecting…"))
                .font(.caption)
                .foregroundStyle(.secondary)
            Spacer()
            Toggle(String(localized: "Follow"), isOn: $follow)
                .toggleStyle(.button)
                .font(.caption)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(Color(.systemGroupedBackground))
    }

    private var logScroll: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    if viewModel.entries.isEmpty {
                        Text(viewModel.errorMessage ?? String(localized: "Waiting for log output…"))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .padding()
                    }
                    ForEach(Array(viewModel.entries.enumerated()), id: \.offset) { index, entry in
                        Text(entry.message)
                            .font(.system(.caption2, design: .monospaced))
                            .foregroundStyle(entry.isError ? Color.serverOffline : Color.primary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .textSelection(.enabled)
                            .id(index)
                    }
                    Color.clear.frame(height: 1).id(bottomAnchor)
                }
                .padding(8)
            }
            .background(Color(.systemBackground))
            .onChange(of: viewModel.entries.count) { _, _ in
                if follow {
                    withAnimation(.linear(duration: 0.1)) { proxy.scrollTo(bottomAnchor, anchor: .bottom) }
                }
            }
        }
    }

    private let bottomAnchor = "docker-logs-bottom"
}
