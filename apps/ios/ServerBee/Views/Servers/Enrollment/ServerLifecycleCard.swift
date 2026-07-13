import SwiftUI

/// Admin-only agent lifecycle actions, shown on the server detail Overview tab.
///
/// - Unclaimed servers: issue or exactly replace an Enrollment offer.
/// - Claimed servers: begin Graceful or Emergency re-enrollment,
///   Upgrade (gated on effective `upgrade` capability + online), and Delete.
///
/// Every mint surfaces the plaintext code + install command once, via a sheet.
struct ServerLifecycleCard: View {
    let serverId: String
    let config: ServerConfig?
    let capabilities: CapabilitySet
    let isOnline: Bool
    let isPending: Bool
    /// Re-fetch the server config after an Agent Authority change.
    var onConfigChanged: () -> Void = {}
    /// Called after a successful delete so the caller can pop the detail screen.
    let onDeleted: () -> Void

    @Environment(\.apiClient) private var apiClient
    @Environment(AuthManager.self) private var authManager
    @Environment(UpgradeJobsStore.self) private var upgradeJobs
    @State private var viewModel = AgentLifecycleViewModel()

    @State private var showReenrollment = false
    @State private var showRevokeAuthority = false
    @State private var showUpgrade = false
    @State private var showDelete = false
    @State private var upgradeQueued = false
    @State private var offerClock = Date()

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
        .task(id: config?.outstandingOffer?.expiresAt) {
            offerClock = Date()
            guard
                let expiresAt = config?.outstandingOffer?.expiresAt,
                let expiry = ISO8601DateFormatter.shared.date(from: expiresAt)
            else { return }
            let remaining = expiry.timeIntervalSinceNow
            guard remaining > 0 else { return }
            try? await Task.sleep(for: .seconds(remaining))
            guard !Task.isCancelled else { return }
            offerClock = Date()
        }
        .confirmationDialog(String(localized: "Agent re-enrollment"), isPresented: $showReenrollment, titleVisibility: .visible) {
            Button(String(localized: "Graceful re-enrollment")) {
                Task {
                    await viewModel.beginReenrollment(
                        serverId: serverId,
                        mode: .graceful,
                        serverUrl: authManager.serverUrl,
                        apiClient: apiClient
                    )
                }
            }
            Button(String(localized: "Emergency re-enrollment"), role: .destructive) {
                Task {
                    await viewModel.beginReenrollment(
                        serverId: serverId,
                        mode: .emergency,
                        serverUrl: authManager.serverUrl,
                        apiClient: apiClient
                    )
                }
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(localized: "Graceful mode preserves current Agent Authority until claim. Emergency mode revokes it and fences the connection immediately."))
        }
        .confirmationDialog(String(localized: "Revoke Agent Authority?"), isPresented: $showRevokeAuthority, titleVisibility: .visible) {
            Button(String(localized: "Revoke Agent Authority"), role: .destructive) {
                Task { await runRevokeAuthority() }
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(localized: "The Agent is fenced immediately and the server becomes unclaimed. No Enrollment offer is created."))
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
            // stepper renders without a real upgrade on the shared demo. Delay
            // past the initial WS full_sync (which carries an empty upgrades
            // snapshot and would otherwise wipe the seeded job).
            if let stage = debugUpgradeStage {
                try? await Task.sleep(for: .seconds(3))
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

        if let expired = expiredOutstandingOffer {
            expiredOfferNotice(expired)
        }

        if let outstanding = activeOutstandingOffer {
            VStack(alignment: .leading, spacing: 4) {
                if let prefix = outstanding.codePrefix {
                    DetailRow(label: String(localized: "Current code"), value: "\(prefix)…", monospaced: true)
                }
                if let expiry = outstanding.expiresAt {
                    DetailRow(label: String(localized: "Expires"), value: Formatters.formatRelativeTime(expiry))
                }
            }
        }

        if let outstanding = activeOutstandingOffer {
            actionButton(
                title: String(localized: "Replace outstanding offer"),
                systemImage: "arrow.triangle.2.circlepath",
                tint: .brandAccent
            ) {
                Task { await runReplace(outstanding) }
            }
        } else {
            actionButton(
                title: String(localized: "Issue enrollment offer"),
                systemImage: "qrcode",
                tint: .brandAccent
            ) {
                Task {
                    await viewModel.issueOffer(
                        serverId: serverId,
                        serverUrl: authManager.serverUrl,
                        apiClient: apiClient
                    )
                }
            }
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

        if let outstanding = activeOutstandingOffer {
            outstandingNotice(outstanding)
        } else {
            if let expired = expiredOutstandingOffer {
                expiredOfferNotice(expired)
            }
            actionButton(
                title: String(localized: "Agent re-enrollment"),
                systemImage: "arrow.triangle.2.circlepath",
                tint: .brandAccent
            ) { showReenrollment = true }
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
            title: String(localized: "Revoke Agent Authority"),
            systemImage: "person.crop.circle.badge.xmark",
            tint: .serverOffline
        ) { showRevokeAuthority = true }

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

    // MARK: - Outstanding Enrollment offer

    private var activeOutstandingOffer: OutstandingOffer? {
        guard let offer = config?.outstandingOffer, !offer.isExpired(at: offerClock) else { return nil }
        return offer
    }

    private var expiredOutstandingOffer: OutstandingOffer? {
        guard let offer = config?.outstandingOffer, offer.isExpired(at: offerClock) else { return nil }
        return offer
    }

    @ViewBuilder
    func expiredOfferNotice(_ offer: OutstandingOffer) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(String(localized: "Enrollment offer expired"), systemImage: "clock.badge.xmark")
                .font(.caption.bold())
                .foregroundStyle(.secondary)
            if let prefix = offer.codePrefix {
                DetailRow(label: String(localized: "Code"), value: "\(prefix)…", monospaced: true)
            }
            Text(String(localized: "This offer is terminal and can no longer be replaced or revoked."))
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.secondary.opacity(0.08))
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }

    @ViewBuilder
    func outstandingNotice(_ outstanding: OutstandingOffer) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(String(localized: "Outstanding enrollment offer"), systemImage: "clock.badge.exclamationmark")
                .font(.caption.bold())
                .foregroundStyle(Color.warningAmber)
            if let prefix = outstanding.codePrefix {
                DetailRow(label: String(localized: "Code"), value: "\(prefix)…", monospaced: true)
            }
            if let expiry = outstanding.expiresAt {
                DetailRow(label: String(localized: "Expires"), value: Formatters.formatRelativeTime(expiry))
            }
            Text(String(localized: "Replace this exact offer if its plaintext code was lost, or revoke it without creating a successor."))
                .font(.caption2)
                .foregroundStyle(.secondary)
            actionButton(
                title: String(localized: "Revoke offer"),
                systemImage: "xmark.circle",
                tint: .serverOffline
            ) { Task { await runRevoke(outstanding) } }
            actionButton(
                title: String(localized: "Replace offer"),
                systemImage: "arrow.triangle.2.circlepath",
                tint: .brandAccent
            ) { Task { await runReplace(outstanding) } }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.warningAmber.opacity(0.12))
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }

    func runRevoke(_ outstanding: OutstandingOffer) async {
        if await viewModel.revokeOffer(serverId: serverId, offerId: outstanding.id, apiClient: apiClient) {
            onConfigChanged()
        }
    }

    func runReplace(_ outstanding: OutstandingOffer) async {
        await viewModel.replaceOffer(
            serverId: serverId,
            offerId: outstanding.id,
            serverUrl: authManager.serverUrl,
            apiClient: apiClient
        )
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

    private func runRevokeAuthority() async {
        if await viewModel.revokeAuthority(serverId: serverId, apiClient: apiClient) {
            onConfigChanged()
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
