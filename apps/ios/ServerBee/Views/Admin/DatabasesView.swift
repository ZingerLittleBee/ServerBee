import SwiftUI

/// GeoIP / ASN database maintenance. Status is readable by members; downloads
/// are admin-only (the trigger buttons are gated on `isAdmin`).
struct DatabasesView: View {
    let isAdmin: Bool
    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = DatabasesViewModel()

    var body: some View {
        List {
            if let message = viewModel.message {
                Section { Label(message, systemImage: "info.circle").font(.subheadline) }
            }
            dbSection(
                title: String(localized: "GeoIP"),
                status: viewModel.geoip,
                downloading: viewModel.downloadingGeoip,
                action: { Task { await viewModel.downloadGeoip(apiClient: apiClient) } }
            )
            dbSection(
                title: String(localized: "ASN"),
                status: viewModel.asn,
                downloading: viewModel.downloadingAsn,
                action: { Task { await viewModel.downloadAsn(apiClient: apiClient) } }
            )
        }
        .overlay { if viewModel.isLoading, viewModel.geoip == nil, viewModel.asn == nil { ProgressView() } }
        .navigationTitle(String(localized: "Databases"))
        .navigationBarTitleDisplayMode(.inline)
        .task { await viewModel.load(apiClient: apiClient) }
        .refreshable { await viewModel.load(apiClient: apiClient) }
    }

    @ViewBuilder
    private func dbSection(title: String, status: DbStatus?, downloading: Bool, action: @escaping () -> Void) -> some View {
        Section(title) {
            if let status {
                DetailRow(
                    label: String(localized: "Status"),
                    value: status.installed ? String(localized: "Installed") : String(localized: "Not installed"),
                    valueColor: status.installed ? .serverOnline : .secondary
                )
                if let source = status.source {
                    DetailRow(label: String(localized: "Source"), value: source.capitalized)
                }
                if let size = status.fileSize {
                    DetailRow(label: String(localized: "Size"), value: Formatters.formatBytes(size))
                }
                if let updated = status.updatedAt {
                    DetailRow(label: String(localized: "Updated"), value: Formatters.formatRelativeTime(updated))
                }
            } else {
                Text(String(localized: "Unavailable")).foregroundStyle(.secondary)
            }

            if isAdmin {
                Button {
                    action()
                } label: {
                    HStack {
                        if downloading { ProgressView() } else { Image(systemName: "arrow.down.circle") }
                        Text(status?.installed == true ? String(localized: "Update Database") : String(localized: "Download Database"))
                    }
                }
                .disabled(downloading)
            }
        }
    }
}
