import SwiftUI

/// Server detail "IP Quality" section: egress IP reputation snapshot, risk
/// flags, geo/ASN, and streaming-service unlock results. Gated by the parent on
/// CAP_IP_QUALITY. Admins can trigger a recheck (an async agent job).
struct ServerIpQualitySection: View {
    let serverId: String
    let isAdmin: Bool
    /// Checks run on the agent, so they need the server online.
    var isOnline = true

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
                if let snapshot = viewModel.data?.ipQuality {
                    IpQualitySnapshotCard(snapshot: snapshot)
                } else {
                    notCheckedCard
                }
                if let results = viewModel.data?.unlockResults, !results.isEmpty {
                    UnlockResultsCard(results: results, serviceNames: viewModel.serviceNames)
                }
                if let message = viewModel.checkError {
                    checkErrorBanner(message)
                }
                if isAdmin {
                    recheckButton
                    if !isOnline {
                        Text(String(localized: "Checks run on the agent, so the server must be online."))
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                    }
                }
            }
        }
    }

    private var recheckButton: some View {
        Button {
            Task { await viewModel.recheck(serverId: serverId, apiClient: apiClient) }
        } label: {
            HStack(spacing: 8) {
                if viewModel.isChecking {
                    ProgressView()
                        .controlSize(.small)
                        .tint(Color.accentColor)
                } else {
                    Image(systemName: "arrow.clockwise")
                        .accessibilityHidden(true)
                }
                Text(checkButtonTitle)
            }
        }
        .buttonStyle(TintedFillButtonStyle())
        .disabled(viewModel.isChecking || !isOnline)
    }

    private var checkButtonTitle: String {
        if viewModel.isChecking { return String(localized: "Checking…") }
        return viewModel.data?.ipQuality == nil ? String(localized: "Check Now") : String(localized: "Recheck Now")
    }

    private func checkErrorBanner(_ message: String) -> some View {
        Label(message, systemImage: "exclamationmark.triangle.fill")
            .font(.subheadline)
            .foregroundStyle(Color.serverOffline)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(12)
            .background(
                Color.serverOffline.opacity(0.1),
                in: RoundedRectangle(cornerRadius: 12, style: .continuous)
            )
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

/// Reputation summary card (risk ring, level, IP, flags) followed by an inset
/// group of the geo/ASN details that have data. Also embedded by the fleet
/// IP-quality view.
struct IpQualitySnapshotCard: View {
    let snapshot: IpQualitySnapshot

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.privacyMode) private var privacyMode

    var body: some View {
        VStack(spacing: 16) {
            reputationCard
            if !detailItems.isEmpty {
                VStack(spacing: 0) {
                    ForEach(detailItems) { item in
                        IpDetailRow(item: item)
                        if item.id != detailItems.last?.id {
                            Divider()
                                .padding(.leading, 16)
                        }
                    }
                }
                .cardSurface(padding: 0)
            }
        }
    }

    private var reputationCard: some View {
        let color = IpRisk.color(snapshot.riskLevel)
        let layout = dynamicTypeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 12))
            : AnyLayout(HStackLayout(alignment: .center, spacing: 14))
        return VStack(alignment: .leading, spacing: 12) {
            Text(String(localized: "IP Reputation"))
                .font(.subheadline.weight(.semibold))
                .accessibilityAddTraits(.isHeader)
            layout {
                IpRiskRing(score: snapshot.riskScore, color: color)
                VStack(alignment: .leading, spacing: 4) {
                    Text(IpRisk.title(snapshot.riskLevel))
                        .font(.title3.bold())
                        .foregroundStyle(color)
                    Text(verbatim: subtitle)
                        .font(.footnote)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                    if !badges.isEmpty {
                        FlexibleWrap(items: badges) { badge in
                            StatusBadge(text: badge.text, color: badge.isFlag ? .warningAmber : .secondary)
                        }
                        .padding(.top, 2)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityElement(children: .combine)
            }
        }
        .cardSurface()
    }

    /// "ip · last checked <relative>", dropping the check time when unknown.
    private var subtitle: String {
        var parts = [snapshot.ip.maskingIPs(privacyMode)]
        if let checked = snapshot.checkedAt {
            let relative = Formatters.formatRelativeTime(checked)
            parts.append(String(localized: "last checked \(relative)"))
        }
        return parts.joined(separator: " · ")
    }

    /// IP type (neutral) followed by the active risk flags (amber); a flag
    /// that repeats the type label is dropped.
    private var badges: [IpBadge] {
        var items: [IpBadge] = []
        let typeLabel = snapshot.ipTypeLabel
        if let typeLabel {
            items.append(IpBadge(text: typeLabel, isFlag: false))
        }
        items += snapshot.flags.filter { $0 != typeLabel }.map { IpBadge(text: $0, isFlag: true) }
        return items
    }

    private var detailItems: [IpDetailItem] {
        var items: [IpDetailItem] = []
        if let loc = snapshot.location {
            items.append(IpDetailItem(label: String(localized: "Location"), value: loc))
        }
        if let asn = snapshot.asn {
            items.append(IpDetailItem(label: "ASN", value: asn, monospaced: true))
        }
        if let org = snapshot.asOrg {
            items.append(IpDetailItem(label: String(localized: "Organization"), value: org))
        }
        if let abuserScore = snapshot.asnAbuserScore {
            items.append(IpDetailItem(label: String(localized: "ASN abuse score"), value: "\(abuserScore)"))
        }
        if let email = snapshot.abuseEmail {
            items.append(IpDetailItem(label: String(localized: "Abuse contact"), value: email))
        }
        return items
    }
}

// MARK: - Unlock results card

/// "Service access" group: one inset row per streaming/unlock service with its
/// region/notes, latency and status badge. Also embedded by the fleet view.
struct UnlockResultsCard: View {
    let results: [UnlockResultDto]
    let serviceNames: [String: String]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            GroupHeader(String(localized: "Service Access"))
            VStack(spacing: 0) {
                ForEach(results) { result in
                    UnlockResultRow(result: result, name: serviceNames[result.serviceId] ?? result.serviceId)
                    if result.id != results.last?.id {
                        Divider()
                            .padding(.leading, 16)
                    }
                }
            }
            .cardSurface(padding: 0)
        }
    }
}

// MARK: - Building blocks

private extension IpRisk {
    /// Headline for the reputation card ("Low risk", …).
    static func title(_ level: String) -> String {
        switch level {
        case "low": String(localized: "Low risk")
        case "medium": String(localized: "Medium risk")
        case "high": String(localized: "High risk")
        case "unknown": String(localized: "Risk unknown")
        default: label(level)
        }
    }
}

private struct IpBadge: Hashable {
    let text: String
    let isFlag: Bool
}

private struct IpDetailItem: Identifiable {
    let label: String
    let value: String
    var monospaced = false

    var id: String { label }
}

/// Risk score inside a ring stroked in the risk-level colour.
private struct IpRiskRing: View {
    let score: Int?
    let color: Color

    @ScaledMetric(relativeTo: .title2) private var diameter: CGFloat = 64
    @ScaledMetric(relativeTo: .title2) private var lineWidth: CGFloat = 5

    var body: some View {
        ZStack {
            Circle()
                .strokeBorder(color, lineWidth: lineWidth)
            Text(score.map { "\($0)" } ?? "—")
                .font(.title2.bold())
                .monospacedDigit()
                .foregroundStyle(score == nil ? AnyShapeStyle(.secondary) : AnyShapeStyle(.primary))
                .lineLimit(1)
                .minimumScaleFactor(0.6)
                .padding(lineWidth + 2)
        }
        .frame(width: diameter, height: diameter)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(String(localized: "Risk score")))
        .accessibilityValue(Text(score.map { "\($0)" } ?? String(localized: "Not available")))
    }
}

/// Label → value inset row; stacks vertically at accessibility text sizes.
private struct IpDetailRow: View {
    let item: IpDetailItem

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        let layout = dynamicTypeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 2))
            : AnyLayout(HStackLayout(alignment: .firstTextBaseline, spacing: 0))
        layout {
            // Side by side, the short label keeps its full width and a long
            // value (e.g. an abuse email) wraps instead of crushing the label.
            Text(item.label)
                .foregroundStyle(.primary)
                .fixedSize(horizontal: !dynamicTypeSize.isAccessibilitySize, vertical: false)
            if !dynamicTypeSize.isAccessibilitySize {
                Spacer(minLength: 12)
            }
            Text(item.value)
                .font(item.monospaced ? .subheadline.monospaced() : .subheadline)
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .multilineTextAlignment(dynamicTypeSize.isAccessibilitySize ? .leading : .trailing)
                .textSelection(.enabled)
        }
        .font(.subheadline)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(item.label))
        .accessibilityValue(Text(item.value))
    }
}

/// One service row: name, region/notes, latency and access status.
private struct UnlockResultRow: View {
    let result: UnlockResultDto
    let name: String

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        let layout = dynamicTypeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 6))
            : AnyLayout(HStackLayout(alignment: .center, spacing: 10))
        layout {
            VStack(alignment: .leading, spacing: 1) {
                Text(name)
                    .font(.callout)
                if let note {
                    Text(verbatim: note)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: 10) {
                if let latency = result.latencyMs {
                    Text(verbatim: "\(latency) ms")
                        .font(.footnote)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                StatusBadge(text: UnlockStatusStyle.label(result.status), color: UnlockStatusStyle.color(result.status))
            }
            .fixedSize()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
        .accessibilityElement(children: .combine)
    }

    /// Detected region plus any agent-reported detail (e.g. a failure reason).
    private var note: String? {
        var parts: [String] = []
        if let region = result.region, !region.isEmpty {
            parts.append(String(localized: "Region \(region)"))
        }
        if let detail = result.detail, !detail.isEmpty {
            parts.append(detail)
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }
}

/// Full-width, large tinted button: accent label on a low-opacity accent fill.
private struct TintedFillButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled
    @ScaledMetric(relativeTo: .headline) private var minHeight: CGFloat = 50

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.headline)
            .foregroundStyle(Color.accentColor)
            .frame(maxWidth: .infinity, minHeight: minHeight)
            .padding(.horizontal, 16)
            .background(
                Color.accentColor.opacity(0.15),
                in: RoundedRectangle(cornerRadius: 14, style: .continuous)
            )
            .contentShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
            .opacity(configuration.isPressed || !isEnabled ? 0.6 : 1)
            .animation(.easeOut(duration: 0.15), value: configuration.isPressed)
    }
}
