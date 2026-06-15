import SwiftUI

/// Admin rate-limit monitor: active buckets (login / register / public) with a
/// reset-all action.
struct RateLimitView: View {
    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = RateLimitViewModel()
    @State private var showReset = false

    var body: some View {
        List {
            if let error = viewModel.loadError {
                Section { Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline) }
            }
            if let status = viewModel.status {
                Section(String(localized: "Limits")) {
                    DetailRow(label: String(localized: "Login / window"), value: "\(status.loginMax) / \(status.authWindowSeconds)s")
                    DetailRow(label: String(localized: "Register / window"), value: "\(status.registerMax) / \(status.authWindowSeconds)s")
                    DetailRow(label: String(localized: "Public / window"), value: "\(status.publicMax) / \(status.publicWindowSeconds)s")
                }

                Section {
                    if status.entries.isEmpty {
                        Text(String(localized: "No active rate-limit buckets.")).foregroundStyle(.secondary)
                    } else {
                        ForEach(status.entries) { bucket in
                            BucketRow(bucket: bucket)
                        }
                    }
                } header: {
                    Text(String(localized: "Active (\(status.entries.count))"))
                }
            }
        }
        .overlay { if viewModel.isLoading, viewModel.status == nil { ProgressView() } }
        .navigationTitle(String(localized: "Rate Limits"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button(String(localized: "Reset")) { showReset = true }
                    .disabled(viewModel.status?.entries.isEmpty ?? true)
            }
        }
        .task { await viewModel.load(apiClient: apiClient) }
        .refreshable { await viewModel.load(apiClient: apiClient) }
        .confirmationDialog(
            String(localized: "Reset all rate limits?"),
            isPresented: $showReset,
            titleVisibility: .visible
        ) {
            Button(String(localized: "Reset All"), role: .destructive) {
                Task { await viewModel.resetAll(apiClient: apiClient) }
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(localized: "Clears every active login, register and public bucket."))
        }
    }
}

private struct BucketRow: View {
    let bucket: RateLimitBucket

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Chip(text: bucket.scope.capitalized, color: bucket.blocked ? .serverOffline : .secondary)
                Text(bucket.ip).font(.subheadline.monospaced())
                Spacer()
                if bucket.blocked {
                    Chip(text: String(localized: "Blocked"), color: .serverOffline)
                }
            }
            HStack {
                Text(String(localized: "\(bucket.count) / \(bucket.max)"))
                    .font(.caption).foregroundStyle(.secondary)
                Spacer()
                if bucket.secondsRemaining > 0 {
                    Text(String(localized: "resets in \(bucket.secondsRemaining)s"))
                        .font(.caption2).foregroundStyle(.tertiary)
                }
            }
        }
        .padding(.vertical, 2)
    }
}
