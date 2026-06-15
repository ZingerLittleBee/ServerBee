import SwiftUI

/// Admin-only agent lifecycle actions, shown on the server detail Overview tab.
///
/// - Pending servers: show the outstanding code summary + "Get install command"
///   (mints a fresh one-time code).
/// - Enrolled servers: Recover (re-mint, optionally revoking the live token),
///   Upgrade (gated on effective `upgrade` capability + online), and Delete.
///
/// Every mint surfaces the plaintext code + install command once, via a sheet.
struct ServerLifecycleCard: View {
    let serverId: String
    let config: ServerConfig?
    let capabilities: CapabilitySet
    let isOnline: Bool
    let isPending: Bool
    /// Re-fetch the server config after an enrollment change (revoke/recover).
    var onConfigChanged: () -> Void = {}
    /// Called after a successful delete so the caller can pop the detail screen.
    let onDeleted: () -> Void

    @Environment(\.apiClient) private var apiClient
    @Environment(AuthManager.self) private var authManager
    @Environment(UpgradeJobsStore.self) private var upgradeJobs
    @State private var viewModel = AgentLifecycleViewModel()

    @State private var showRecover = false
    @State private var showUpgrade = false
    @State private var showDelete = false
    @State private var upgradeQueued = false

    var body: some View {
        SectionCard(String(localized: "Agent"), systemImage: "gearshape.2") {
            VStack(alignment: .leading, spacing: 12) {
                if isPending {
                    pendingContent
                } else {
                    enrolledContent
                }

                if upgradeQueued, upgradeJobs.job(forServer: serverId) == nil {
                    Label(String(localized: "Upgrade requested — the agent will reconnect shortly."),
                          systemImage: "checkmark.circle.fill")
                        .font(.caption)
                        .foregroundStyle(Color.serverOnline)
                }
                if let error = viewModel.errorMessage {
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .font(.caption)
                        .foregroundStyle(Color.serverOffline)
                }
            }
        }
        .sheet(item: $viewModel.issued) { issued in
            issuedSheet(issued)
        }
        .task {
            if !isPending, capabilities.isEnabled(.upgrade) {
                await viewModel.loadLatestVersion(apiClient: apiClient)
            }
        }
        .confirmationDialog(String(localized: "Recover agent"), isPresented: $showRecover, titleVisibility: .visible) {
            Button(String(localized: "Generate new code")) {
                Task { await viewModel.recover(serverId: serverId, revokeImmediately: false, serverUrl: authManager.serverUrl, apiClient: apiClient) }
            }
            Button(String(localized: "Revoke token & generate code"), role: .destructive) {
                Task { await viewModel.recover(serverId: serverId, revokeImmediately: true, serverUrl: authManager.serverUrl, apiClient: apiClient) }
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(localized: "Generates a new enrollment code for this server. Revoking immediately disconnects the current agent until it re-enrolls with the new code."))
        }
        .confirmationDialog(String(localized: "Upgrade agent"), isPresented: $showUpgrade, titleVisibility: .visible) {
            if let target = viewModel.latestVersion {
                Button(String(format: String(localized: "Upgrade to v%@"), target)) {
                    Task { await runUpgrade(to: target) }
                }
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(localized: "Triggers the agent to download and install the latest release, then reconnect. The agent restarts during the upgrade."))
        }
        .confirmationDialog(String(localized: "Delete server?"), isPresented: $showDelete, titleVisibility: .visible) {
            Button(String(localized: "Delete"), role: .destructive) { Task { await runDelete() } }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(localized: "Permanently removes this server and its history. The agent will be disconnected. This cannot be undone."))
        }
        #if DEBUG
        .task {
            // Visual-verification hook: preview the issued-code sheet without a
            // real write on the shared demo backend.
            if UITestSupport.autoPresent == "enroll-preview" {
                viewModel.issued = AgentLifecycleViewModel.IssuedEnrollment(
                    id: "preview",
                    code: "SBENROLL-9F2A-7C41-DE08",
                    expiresAt: "2026-06-15T18:00:00Z",
                    installCommand: AgentLifecycleViewModel.installCommand(
                        code: "SBENROLL-9F2A-7C41-DE08",
                        serverUrl: authManager.serverUrl
                    )
                )
            }
            // Visual-verification hook: seed a fake live upgrade job so the
            // stepper renders without a real upgrade on the shared demo.
            if let stage = debugUpgradeStage {
                upgradeJobs.setJobs([
                    UpgradeJob(
                        serverId: serverId,
                        jobId: "preview-job",
                        targetVersion: "1.9.0",
                        stage: stage,
                        status: .running,
                        error: nil,
                        backupPath: nil,
                        startedAt: "2026-06-15T18:00:00Z",
                        finishedAt: nil
                    )
                ])
            }
        }
        #endif
    }
}

private extension ServerLifecycleCard {

    // MARK: - Pending

    @ViewBuilder
    var pendingContent: some View {
        Text(String(localized: "This server has no connected agent yet. Generate a one-time code and run the install command on the host."))
            .font(.caption)
            .foregroundStyle(.secondary)

        if let outstanding = config?.outstandingEnrollment {
            VStack(alignment: .leading, spacing: 4) {
                if let prefix = outstanding.codePrefix {
                    DetailRow(label: String(localized: "Current code"), value: "\(prefix)…", monospaced: true)
                }
                if let expiry = outstanding.expiresAt {
                    DetailRow(label: String(localized: "Expires"), value: Formatters.formatRelativeTime(expiry))
                }
            }
        }

        actionButton(
            title: String(localized: "Get install command"),
            systemImage: "qrcode",
            tint: .brandAccent
        ) {
            Task { await viewModel.regenerateCode(serverId: serverId, serverUrl: authManager.serverUrl, apiClient: apiClient) }
        }
    }

    // MARK: - Enrolled

    @ViewBuilder
    private var enrolledContent: some View {
        if let current = config?.agentVersion {
            DetailRow(label: String(localized: "Agent version"), value: "v\(current)", monospaced: true)
        }
        if hasUpdate, let target = viewModel.latestVersion {
            Label(String(format: String(localized: "Update available: v%@"), target), systemImage: "arrow.up.circle.fill")
                .font(.caption)
                .foregroundStyle(Color.brandAccent)
        }

        if let outstanding = config?.outstandingEnrollment {
            outstandingNotice(outstanding)
        } else {
            actionButton(
                title: String(localized: "Recover agent"),
                systemImage: "arrow.triangle.2.circlepath",
                tint: .brandAccent
            ) { showRecover = true }
        }

        if capabilities.isEnabled(.upgrade) {
            actionButton(
                title: String(localized: "Upgrade agent"),
                systemImage: "arrow.up.circle",
                tint: .brandAccent,
                disabled: !isOnline || !hasUpdate || isUpgradeRunning,
                disabledNote: isUpgradeRunning ? String(localized: "Upgrading…") : upgradeNote
            ) { showUpgrade = true }
        }

        if let job = upgradeJobs.job(forServer: serverId) {
            UpgradeStepperView(job: job)
        }

        Divider()

        actionButton(
            title: String(localized: "Delete server"),
            systemImage: "trash",
            tint: .serverOffline
        ) { showDelete = true }
    }

    /// True when the server reports an agent version that differs from the
    /// latest released version (mirrors the web "has update" check).
    private var hasUpdate: Bool {
        guard let current = config?.agentVersion, let latest = viewModel.latestVersion else { return false }
        return current != latest
    }

    private var upgradeNote: String? {
        if !isOnline { return String(localized: "Agent offline") }
        if !hasUpdate { return String(localized: "Up to date") }
        return nil
    }

    /// True while a live upgrade job for this server is still running.
    private var isUpgradeRunning: Bool {
        upgradeJobs.job(forServer: serverId)?.status == .running
    }

    // MARK: - Outstanding enrollment (recover gate)

    @ViewBuilder
    func outstandingNotice(_ outstanding: OutstandingEnrollment) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(String(localized: "Pending enrollment code"), systemImage: "clock.badge.exclamationmark")
                .font(.caption.bold())
                .foregroundStyle(Color.warningAmber)
            if let prefix = outstanding.codePrefix {
                DetailRow(label: String(localized: "Code"), value: "\(prefix)…", monospaced: true)
            }
            if let expiry = outstanding.expiresAt {
                DetailRow(label: String(localized: "Expires"), value: Formatters.formatRelativeTime(expiry))
            }
            Text(String(localized: "Revoke the pending code before generating a new one."))
                .font(.caption2)
                .foregroundStyle(.secondary)
            actionButton(
                title: String(localized: "Revoke pending code"),
                systemImage: "xmark.circle",
                tint: .serverOffline
            ) { Task { await runRevoke(outstanding) } }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.warningAmber.opacity(0.12))
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }

    func runRevoke(_ outstanding: OutstandingEnrollment) async {
        if await viewModel.revokeEnrollment(enrollmentId: outstanding.id, apiClient: apiClient) {
            onConfigChanged()
        }
    }

    // MARK: - Action row

    @ViewBuilder
    private func actionButton(
        title: String,
        systemImage: String,
        tint: Color,
        disabled: Bool = false,
        disabledNote: String? = nil,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: systemImage)
                    .frame(width: 22)
                Text(title)
                Spacer()
                if viewModel.isWorking {
                    ProgressView()
                } else if let disabledNote {
                    Text(disabledNote)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    Image(systemName: "chevron.right")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
            }
            .foregroundStyle(disabled ? AnyShapeStyle(.secondary) : AnyShapeStyle(tint))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(disabled || viewModel.isWorking)
    }

    // MARK: - Issued sheet

    private func issuedSheet(_ issued: AgentLifecycleViewModel.IssuedEnrollment) -> some View {
        NavigationStack {
            ScrollView {
                EnrollmentResultView(issued: issued)
                    .padding()
            }
            .background(Color(.systemGroupedBackground))
            .navigationTitle(String(localized: "Enrollment code"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(String(localized: "Done")) {
                        viewModel.issued = nil
                        onConfigChanged()
                    }
                }
            }
        }
    }

    // MARK: - Actions

    private func runUpgrade(to version: String) async {
        let error = await viewModel.upgrade(serverId: serverId, version: version, apiClient: apiClient)
        upgradeQueued = error == nil
    }

    private func runDelete() async {
        if await viewModel.delete(serverId: serverId, apiClient: apiClient) {
            onDeleted()
        }
    }

    #if DEBUG
    /// Parses the visual-verification hook `upgrade-progress[:<stage>]` into a
    /// stage (defaults to `.installing`). Returns nil when the hook is absent.
    private var debugUpgradeStage: UpgradeStage? {
        guard let raw = UITestSupport.autoPresent, raw.hasPrefix("upgrade-progress") else { return nil }
        let parts = raw.split(separator: ":", maxSplits: 1)
        if parts.count == 2, let stage = UpgradeStage(rawValue: String(parts[1])) { return stage }
        return .installing
    }
    #endif
}
