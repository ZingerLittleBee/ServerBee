import SwiftUI

/// Cross-server IP-quality overview: each server's egress IP reputation at a
/// glance (risk level, flags, location), expanding in place to the full snapshot.
/// Read-only; rechecking stays on the per-server detail screen.
struct FleetIpQualityView: View {
    @Environment(\.apiClient) private var apiClient
    @Environment(ServersViewModel.self) private var serversViewModel
    @State private var viewModel = FleetIpQualityViewModel()

    private var namesById: [String: String] {
        Dictionary(serversViewModel.servers.map { ($0.id, $0.name) }, uniquingKeysWith: { a, _ in a })
    }

    var body: some View {
        ScrollView {
            content
                .padding(16)
        }
        .background(Color(.systemGroupedBackground))
        .navigationTitle(String(localized: "IP Quality"))
        .navigationBarTitleDisplayMode(.inline)
        .refreshable { await viewModel.reload(apiClient: apiClient) }
        .task { await viewModel.loadIfNeeded(apiClient: apiClient) }
    }

    @ViewBuilder
    private var content: some View {
        if viewModel.isLoading && viewModel.servers.isEmpty {
            ProgressView().frame(maxWidth: .infinity).padding(.top, 80)
        } else if let error = viewModel.loadError, viewModel.servers.isEmpty {
            errorState(error)
        } else if viewModel.servers.isEmpty {
            emptyState
        } else {
            VStack(spacing: 12) {
                ForEach(viewModel.servers, id: \.serverId) { data in
                    FleetIpQualityRow(
                        name: namesById[data.serverId] ?? data.serverId,
                        data: data,
                        serviceNames: viewModel.serviceNames
                    )
                }
            }
        }
    }

    private var emptyState: some View {
        ContentUnavailableView(
            String(localized: "No IP quality data"),
            systemImage: "shield.slash",
            description: Text(String(localized: "No server has reported egress IP quality yet."))
        )
        .padding(.top, 60)
    }

    private func errorState(_ message: String) -> some View {
        ContentUnavailableView {
            Label(String(localized: "IP quality unavailable"), systemImage: "shield.slash")
        } description: {
            Text(message)
        } actions: {
            Button(String(localized: "Retry")) { Task { await viewModel.reload(apiClient: apiClient) } }
        }
        .padding(.top, 60)
    }
}

/// Per-server IP-quality summary card. Servers with a snapshot or unlock
/// results expand in place; the detail cards follow the header as full-width
/// cards of their own rather than nesting inside it.
private struct FleetIpQualityRow: View {
    let name: String
    let data: ServerIpQualityData
    let serviceNames: [String: String]

    @Environment(\.privacyMode) private var privacyMode
    @State private var isExpanded = false

    private var hasDetail: Bool { data.ipQuality != nil || !data.unlockResults.isEmpty }

    var body: some View {
        VStack(spacing: 8) {
            if hasDetail {
                Button {
                    withAnimation(.snappy) { isExpanded.toggle() }
                } label: {
                    header.cardSurface()
                }
                .buttonStyle(.plain)
                .accessibilityValue(Text(isExpanded ? String(localized: "Expanded") : String(localized: "Collapsed")))
            } else {
                header.cardSurface()
            }
            if isExpanded {
                if let snapshot = data.ipQuality {
                    IpQualitySnapshotCard(snapshot: snapshot)
                }
                if !data.unlockResults.isEmpty {
                    UnlockResultsCard(results: data.unlockResults, serviceNames: serviceNames)
                }
            }
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 3) {
                Text(name).font(.subheadline.weight(.medium))
                if let snapshot = data.ipQuality {
                    HStack(spacing: 6) {
                        Text(snapshot.ip.maskingIPs(privacyMode)).font(.caption.monospaced()).foregroundStyle(.secondary)
                        if let loc = snapshot.location {
                            Text(loc).font(.caption2).foregroundStyle(.tertiary).lineLimit(1)
                        }
                    }
                } else {
                    Text(String(localized: "Not checked")).font(.caption).foregroundStyle(.tertiary)
                }
            }
            Spacer(minLength: 8)
            if let snapshot = data.ipQuality {
                riskBadge(snapshot)
            }
            if hasDetail {
                Image(systemName: "chevron.right")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.tertiary)
                    .rotationEffect(.degrees(isExpanded ? 90 : 0))
                    .accessibilityHidden(true)
            }
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    private func riskBadge(_ snapshot: IpQualitySnapshot) -> some View {
        let color = IpRisk.color(snapshot.riskLevel)
        return Text(IpRisk.label(snapshot.riskLevel))
            .font(.caption2.weight(.semibold))
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .foregroundStyle(color)
            .background(color.opacity(0.14))
            .clipShape(Capsule())
    }
}
