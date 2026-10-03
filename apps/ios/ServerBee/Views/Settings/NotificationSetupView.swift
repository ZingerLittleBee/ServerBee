import SwiftUI
import UIKit
import UserNotifications

struct NotificationSetupView: View {
    @Environment(PushNotificationManager.self) private var manager
    @State private var draft = PushPreferences()

    var body: some View {
        Form {
            Section {
                Text(LocalizedStringKey(
                    "Your Server selects recipients. The Push Relay sees device tokens, source IPs, timing, request sizes, " +
                    "environment, grant identifiers and encrypted content. It cannot read notification content or content keys."
                ))
                Text("This build supports encrypted tests and administrator security rule notifications. Alert and task delivery is still being implemented.")
                    .foregroundStyle(.secondary)
            } header: { Text("Mobile notification privacy") }

            Section("Subscriptions") {
                Toggle("Alerts and recoveries", isOn: $draft.alerts)
                if manager.confirmed?.securityAllowed == true {
                    Toggle("Security rule matches", isOn: $draft.security)
                }
                Toggle("Final task failures", isOn: $draft.taskFailure)
                Toggle("Successful task runs", isOn: $draft.taskSuccess)
                if manager.confirmed?.preferences.enabled == true {
                    Button("Save subscriptions") { Task { await manager.savePreferences(draft) } }
                    Button("Disable notifications", role: .destructive) {
                        var disabled = draft
                        disabled.enabled = false
                        Task { await manager.savePreferences(disabled) }
                    }
                } else {
                    Button("Enable notifications") {
                        var enabled = draft
                        enabled.enabled = true
                        Task { await manager.savePreferences(enabled) }
                    }
                }
            }
            .disabled(manager.isSaving || manager.confirmed == nil)

            Section("Test notification") {
                Button("Send test notification") { Task { await manager.sendTestNotification() } }
                    .disabled(manager.isSaving || manager.isTesting || manager.confirmed?.registered != true)
                if manager.isTesting { ProgressView() }
                Button("Refresh test status") { Task { await manager.refreshTestStatus() } }
                    .disabled(manager.isTesting)
                if let result = manager.testResult {
                    Text(result.statusMessage)
                        .foregroundStyle(.secondary)
                    Text("Delivery status does not guarantee device presentation or exactly-once display.")
                        .foregroundStyle(.secondary)
                }
            }

            Section("Setup status") {
                LabeledContent("System permission", value: permissionLabel)
                LabeledContent("Server registration", value: registrationLabel)
                if manager.isSaving { ProgressView() }
                if let error = manager.errorMessage { Text(error).foregroundStyle(.red) }
                if manager.verificationUnavailable {
                    Text("Verified push is unavailable. Monitoring and login still work.")
                }
                Button("Retry notification setup") { Task { await manager.retry() } }
                    .disabled(manager.isSaving)
                Button("Open notification settings") {
                    if let url = URL(string: UIApplication.openNotificationSettingsURLString) { UIApplication.shared.open(url) }
                }
            }
        }
        .navigationTitle("Notifications")
        .task {
            await manager.reconcile()
            refreshDraft()
            await manager.refreshTestStatus()
            while !Task.isCancelled {
                do { try await Task.sleep(for: .seconds(2)) } catch { break }
                if manager.testResult?.isPending == true { await manager.refreshTestStatus() }
            }
        }
        .onChange(of: manager.confirmed?.revision) { _, _ in
            refreshDraft()
        }
        .onChange(of: manager.confirmed?.securityAllowed) { _, _ in refreshDraft() }
    }

    private func refreshDraft() {
        guard let confirmed = manager.confirmed else { return }
        draft = manager.unconfirmedPreferences ?? confirmed.preferences
        if !confirmed.securityAllowed { draft.security = false }
    }

    private var permissionLabel: String {
        switch manager.authorizationStatus {
        case .authorized, .provisional, .ephemeral: String(localized: "Allowed")
        case .denied: String(localized: "Disabled")
        case .notDetermined: String(localized: "Not requested")
        @unknown default: String(localized: "Unknown")
        }
    }

    private var registrationLabel: String {
        if manager.errorMessage != nil { return String(localized: "Setup failed") }
        if manager.confirmed?.registered == true { return String(localized: "Setup confirmed") }
        if manager.confirmed?.preferences.enabled == true { return String(localized: "Awaiting verification") }
        return String(localized: "Disabled")
    }
}
