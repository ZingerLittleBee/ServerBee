import SwiftUI

/// Admin-only alerting configuration: notification channels and alert rules.
/// Mobile keeps this read-mostly — you can review every rule/channel, flip its
/// enabled state, and fire a channel test — while authoring of the threshold
/// logic itself stays on the web dashboard. The whole `notification`/`alert`
/// API is admin-gated, so the entry point must already be admin-only.
struct AlertConfigView: View {
    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = NotificationsViewModel()

    var body: some View {
        List {
            if let error = viewModel.errorMessage {
                Section {
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(Color.serverOffline)
                }
            }
            rulesSection
            channelsSection
            footerSection
        }
        .overlay {
            if viewModel.isLoading, !viewModel.hasLoaded { ProgressView() }
        }
        .navigationTitle(String(localized: "Alert Config"))
        .navigationBarTitleDisplayMode(.inline)
        .task {
            #if DEBUG
            if UITestSupport.autoPresent == "alert-config" {
                NotificationSampleData.populate(viewModel)
                return
            }
            #endif
            if !viewModel.hasLoaded { await viewModel.load(apiClient: apiClient) }
        }
        .refreshable { await viewModel.load(apiClient: apiClient) }
    }

    // MARK: - Alert rules

    private var rulesSection: some View {
        Section {
            if viewModel.rules.isEmpty, viewModel.hasLoaded {
                Text(String(localized: "No alert rules configured."))
                    .foregroundStyle(.secondary)
            }
            ForEach(viewModel.rules) { rule in
                ruleRow(rule)
            }
        } header: {
            Text(String(localized: "Alert rules"))
        } footer: {
            Text(String(localized: "Rules evaluate live metrics and fire when thresholds are crossed. Edit conditions on the web dashboard."))
        }
    }

    private func ruleRow(_ rule: AlertRule) -> some View {
        let isBusy = viewModel.togglingIds.contains(rule.id)
        return Toggle(isOn: enabledBinding(
            isOn: rule.enabled,
            action: { await viewModel.toggleRule(rule, apiClient: apiClient) }
        )) {
            VStack(alignment: .leading, spacing: 3) {
                Text(rule.name).font(.body)
                HStack(spacing: 6) {
                    Label(rule.coverLabel, systemImage: "scope")
                    if rule.triggerMode == "all" {
                        Text(verbatim: "·")
                        Text(String(localized: "All conditions"))
                    }
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
        }
        .disabled(isBusy)
    }

    // MARK: - Notification channels

    private var channelsSection: some View {
        Section {
            if viewModel.channels.isEmpty, viewModel.hasLoaded {
                Text(String(localized: "No notification channels configured."))
                    .foregroundStyle(.secondary)
            }
            ForEach(viewModel.channels) { channel in
                channelRow(channel)
            }
        } header: {
            Text(String(localized: "Notification channels"))
        } footer: {
            Text(String(localized: "Swipe a channel to send a test notification."))
        }
    }

    private func channelRow(_ channel: NotificationChannel) -> some View {
        let isBusy = viewModel.togglingIds.contains(channel.id)
        let isTesting = viewModel.testingChannelIds.contains(channel.id)
        let result = viewModel.testResults[channel.id]
        return Toggle(isOn: enabledBinding(
            isOn: channel.enabled,
            action: { await viewModel.toggleChannel(channel, apiClient: apiClient) }
        )) {
            HStack(spacing: 12) {
                Image(systemName: channel.typeIcon)
                    .frame(width: 22)
                    .foregroundStyle(Color.brandAccent)
                VStack(alignment: .leading, spacing: 3) {
                    Text(channel.name).font(.body)
                    HStack(spacing: 6) {
                        Text(channel.typeLabel)
                        if isTesting {
                            Text(verbatim: "·")
                            ProgressView().controlSize(.mini)
                        } else if let result {
                            Text(verbatim: "·")
                            Text(result)
                        }
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
            }
        }
        .disabled(isBusy)
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            Button {
                Task { await viewModel.testChannel(channel, apiClient: apiClient) }
            } label: {
                Label(String(localized: "Test"), systemImage: "paperplane")
            }
            .tint(.brandAccent)
            .disabled(isTesting)
        }
    }

    // MARK: - Footer

    private var footerSection: some View {
        Section {
            Label(String(localized: "Changes apply immediately. Full rule and channel authoring lives in the web dashboard."),
                  systemImage: "info.circle")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    // MARK: - Helpers

    /// Builds a `Toggle` binding whose setter dispatches an async mutation. The
    /// getter reflects the model's current value, so the row snaps back if the
    /// request fails (the view model only commits on success).
    private func enabledBinding(isOn: Bool, action: @escaping () async -> Void) -> Binding<Bool> {
        Binding(
            get: { isOn },
            set: { _ in Task { await action() } }
        )
    }
}

#if DEBUG
/// Sample data so the alert-config screen can be visually verified even when the
/// connected server has no channels/rules.
enum NotificationSampleData {
    @MainActor
    static func populate(_ viewModel: NotificationsViewModel) {
        viewModel.hasLoaded = true
        viewModel.rules = [
            decode(AlertRule.self, """
            {"id":"r1","name":"High CPU","enabled":true,"trigger_mode":"any",
             "notification_group_id":"g1","cover_type":"all","server_ids_json":null,
             "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}
            """),
            decode(AlertRule.self, """
            {"id":"r2","name":"Disk almost full","enabled":false,"trigger_mode":"all",
             "notification_group_id":null,"cover_type":"include","server_ids_json":"[\\"s1\\"]",
             "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}
            """)
        ].compactMap { $0 }
        viewModel.channels = [
            decode(NotificationChannel.self, """
            {"id":"c1","name":"Ops Telegram","notify_type":"telegram","config_json":"{}",
             "enabled":true,"created_at":"2026-01-01T00:00:00Z"}
            """),
            decode(NotificationChannel.self, """
            {"id":"c2","name":"On-call webhook","notify_type":"webhook","config_json":"{}",
             "enabled":true,"created_at":"2026-01-01T00:00:00Z"}
            """),
            decode(NotificationChannel.self, """
            {"id":"c3","name":"Email digest","notify_type":"email","config_json":"{}",
             "enabled":false,"created_at":"2026-01-01T00:00:00Z"}
            """)
        ].compactMap { $0 }
    }

    private static func decode<T: Decodable>(_ type: T.Type, _ json: String) -> T? {
        try? JSONDecoder.snakeCase.decode(T.self, from: Data(json.utf8))
    }
}
#endif
