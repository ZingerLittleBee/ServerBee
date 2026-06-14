import SwiftUI

/// Signed-in mobile devices. The current device is flagged and revoking it is
/// effectively a remote sign-out, so it carries an extra warning.
struct DevicesView: View {
    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = DevicesViewModel()
    @State private var pendingRevoke: MobileDevice?

    var body: some View {
        List {
            if let error = viewModel.actionError {
                Section {
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(Color.serverOffline)
                }
            }
            if viewModel.devices.isEmpty, !viewModel.isLoading {
                Section { Text(String(localized: "No active devices.")).foregroundStyle(.secondary) }
            }
            ForEach(viewModel.devices) { device in
                deviceRow(device)
                    .swipeActions {
                        Button(role: .destructive) { pendingRevoke = device } label: {
                            Label(String(localized: "Sign Out"), systemImage: "iphone.slash")
                        }
                    }
            }
        }
        .overlay {
            if viewModel.isLoading, viewModel.devices.isEmpty { ProgressView() }
        }
        .navigationTitle(String(localized: "Devices"))
        .navigationBarTitleDisplayMode(.inline)
        .task { await viewModel.load(apiClient: apiClient) }
        .refreshable { await viewModel.load(apiClient: apiClient) }
        .confirmationDialog(
            confirmTitle,
            isPresented: Binding(get: { pendingRevoke != nil }, set: { if !$0 { pendingRevoke = nil } }),
            titleVisibility: .visible
        ) {
            if let device = pendingRevoke {
                Button(String(localized: "Sign Out"), role: .destructive) {
                    Task { await viewModel.revoke(id: device.id, apiClient: apiClient) }
                }
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            if let device = pendingRevoke, viewModel.isCurrent(device) {
                Text(String(localized: "This is the device you're using now — you'll be signed out of the app."))
            } else {
                Text(String(localized: "That device will need to sign in again."))
            }
        }
    }

    private var confirmTitle: String {
        if let device = pendingRevoke { return String(localized: "Sign out \(device.deviceName)?") }
        return String(localized: "Sign out device?")
    }

    private func deviceRow(_ device: MobileDevice) -> some View {
        HStack(spacing: 12) {
            Image(systemName: "iphone")
                .font(.title3)
                .foregroundStyle(.secondary)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(device.deviceName).font(.body)
                    if viewModel.isCurrent(device) {
                        Chip(text: String(localized: "This device"), color: .brandAccent)
                    }
                }
                Text(String(localized: "Last used \(Formatters.formatRelativeTime(device.lastUsedAt))"))
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 2)
    }
}
