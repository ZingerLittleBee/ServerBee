import SwiftUI

/// Server detail "IP Quality" section: egress IP reputation snapshot, risk
/// flags, geo/ASN, and streaming-service unlock results. Gated by the parent on
/// CAP_IP_QUALITY. Admins can trigger a recheck (an async agent job).
struct ServerIpQualitySection: View {
    let serverId: String
    let isAdmin: Bool

    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = ServerIpQualityViewModel()

    var body: some View {
        ScrollView {
            content
                .padding(16)
        }
        .background(Color(.systemGroupedBackground))
        .scrollIndicators(.hidden)
        .refreshable { await viewModel.reload(serverId: serverId, apiClient: apiClient) }
        .task { await viewModel.loadIfNeeded(serverId: serverId, apiClient: apiClient) }
    }

    @ViewBuilder
    private var content: some View {
        if viewModel.isLoading && viewModel.data == nil {
            loadingState
        } else if let error = viewModel.loadError, viewModel.data == nil {
            errorState(error)
        } else {
            VStack(spacing: 16) {
                if isAdmin {
                    recheckButton
                }
                if let message = viewModel.checkError {
                    Label(message, systemImage: "exclamationmark.triangle.fill")
                        .font(.subheadline)
                        .foregroundStyle(Color.serverOffline)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12)
                        .background(Color.serverOffline.opacity(0.1))
                        .clipShape(RoundedRectangle(cornerRadius: 10))
                }
                if let snapshot = viewModel.data?.ipQuality {
                    IpQualitySnapshotCard(snapshot: snapshot)
                } else {
                    notCheckedCard
                }
                if let results = viewModel.data?.unlockResults, !results.isEmpty {
                    UnlockResultsCard(results: results, serviceNames: viewModel.serviceNames)
                }
            }
        }
    }

    private var recheckButton: some View {
        Button {
            Task { await viewModel.recheck(serverId: serverId, apiClient: apiClient) }
        } label: {
            HStack {
                if viewModel.isChecking {
                    ProgressView().controlSize(.small)
                }
                Text(viewModel.isChecking ? String(localized: "Checking…") : String(localized: "Recheck Now"))
                    .frame(maxWidth: .infinity)
            }
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
        .disabled(viewModel.isChecking)
    }

    private var notCheckedCard: some View {
        SectionCard {
            ContentUnavailableView(
                String(localized: "Not checked yet"),
                systemImage: "shield.slash",
                description: Text(isAdmin
                    ? String(localized: "Run a check to assess this server's IP reputation.")
                    : String(localized: "No IP quality data is available for this server."))
            )
        }
    }

    private var loadingState: some View {
        VStack(spacing: 12) {
            ProgressView()
            Text(String(localized: "Loading IP quality…"))
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 80)
    }

    private func errorState(_ message: String) -> some View {
        ContentUnavailableView {
            Label(String(localized: "IP quality unavailable"), systemImage: "shield.slash")
        } description: {
            Text(message)
        } actions: {
            Button(String(localized: "Retry")) {
                Task { await viewModel.reload(serverId: serverId, apiClient: apiClient) }
            }
        }
        .padding(.top, 60)
    }
}

// MARK: - Snapshot card

struct IpQualitySnapshotCard: View {
    let snapshot: IpQualitySnapshot

    var body: some View {
        SectionCard(String(localized: "IP Reputation"), systemImage: "shield.checkered") {
            VStack(alignment: .leading, spacing: 14) {
                HStack(alignment: .firstTextBaseline) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(snapshot.ip)
                            .font(.headline.monospaced())
                        Text(snapshot.ipType.capitalized)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    riskBadge
                }

                if !snapshot.flags.isEmpty {
                    FlowChips(items: snapshot.flags) { flag in
                        Chip(text: flag, systemImage: "exclamationmark.shield", color: .warningAmber)
                    }
                }

                Divider()

                if let loc = snapshot.location {
                    DetailRow(label: String(localized: "Location"), value: loc, systemImage: "mappin.and.ellipse")
                }
                if let asn = snapshot.asn {
                    DetailRow(label: "ASN", value: asn, systemImage: "network", monospaced: true)
                }
                if let org = snapshot.asOrg {
                    DetailRow(label: String(localized: "Organization"), value: org)
                }
                if let abuserScore = snapshot.asnAbuserScore {
                    DetailRow(label: String(localized: "ASN abuse score"), value: "\(abuserScore)")
                }
                if let email = snapshot.abuseEmail {
                    DetailRow(label: String(localized: "Abuse contact"), value: email, monospaced: true)
                }
                if let checked = snapshot.checkedAt {
                    DetailRow(label: String(localized: "Last checked"), value: Formatters.formatRelativeTime(checked))
                }
            }
        }
    }

    private var riskBadge: some View {
        let color = IpRisk.color(snapshot.riskLevel)
        return VStack(spacing: 2) {
            Text(snapshot.riskScore.map { "\($0)" } ?? "—")
                .font(.title2.bold().monospacedDigit())
                .foregroundStyle(color)
            Text(IpRisk.label(snapshot.riskLevel))
                .font(.caption2.weight(.semibold))
                .foregroundStyle(color)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(color.opacity(0.12))
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}

// MARK: - Unlock results card

struct UnlockResultsCard: View {
    let results: [UnlockResultDto]
    let serviceNames: [String: String]

    var body: some View {
        SectionCard(String(localized: "Service Access"), systemImage: "globe") {
            VStack(spacing: 0) {
                ForEach(results) { result in
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(serviceNames[result.serviceId] ?? result.serviceId)
                                .font(.subheadline)
                            if let region = result.region, !region.isEmpty {
                                Text(region)
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        Spacer()
                        if let latency = result.latencyMs {
                            Text("\(latency) ms")
                                .font(.caption.monospacedDigit())
                                .foregroundStyle(.secondary)
                        }
                        statusBadge(result.status)
                    }
                    .padding(.vertical, 8)
                    if result.id != results.last?.id {
                        Divider()
                    }
                }
            }
        }
    }

    private func statusBadge(_ status: String) -> some View {
        let color = UnlockStatusStyle.color(status)
        return Text(UnlockStatusStyle.label(status))
            .font(.caption2.weight(.semibold))
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .foregroundStyle(color)
            .background(color.opacity(0.14))
            .clipShape(Capsule())
    }
}
