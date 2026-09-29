import SwiftUI

struct AlertDetailView: View {
    let alertKey: String
    @State private var viewModel = AlertDetailViewModel()
    @Environment(\.apiClient) private var apiClient
    @ScaledMetric(relativeTo: .headline) private var buttonHeight: CGFloat = 50

    var body: some View {
        // A real container (not `Group`): `Group` applies `.task` to each child,
        // so the fetch would never start while it has no child and would restart
        // on every branch switch. A refetch on re-appear (e.g. back from the
        // server screen) keeps an already-loaded detail on screen.
        ZStack {
            if let detail = viewModel.detail {
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        header(detail)
                        infoGroup(detail)
                        if !detail.message.isEmpty {
                            messageGroup(detail.message)
                        }
                        viewServerButton(serverId: detail.serverId)
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                }
            } else if let errorMessage = viewModel.errorMessage {
                ContentUnavailableView(errorMessage, systemImage: "exclamationmark.triangle")
            } else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Color(.systemGroupedBackground))
        .navigationTitle(String(localized: "Alert Detail"))
        .navigationBarTitleDisplayMode(.inline)
        .task {
            await viewModel.fetchDetail(alertKey: alertKey, apiClient: apiClient)
        }
    }
}

private extension AlertDetailView {
    // MARK: - Header

    func header(_ detail: MobileAlertDetail) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                AlertStatusBadge(status: detail.status)
                if detail.triggerCount > 1 {
                    AlertTagCapsule.count(detail.triggerCount)
                        .accessibilityLabel(Text(String(format: String(localized: "Triggered %d times"), detail.triggerCount)))
                }
            }
            Text(detail.ruleName)
                .font(.title.bold())
                .accessibilityAddTraits(.isHeader)
            Text(verbatim: subtitle(detail))
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 4)
    }

    /// "<server> · firing for 12 min" / "<server> · resolved 5 min. ago".
    func subtitle(_ detail: MobileAlertDetail) -> String {
        var parts = [detail.serverName]
        switch detail.status {
        case .firing:
            if let started = ISO8601DateFormatter.shared.date(from: detail.firstTriggeredAt) {
                let elapsed = max(Date().timeIntervalSince(started), 60)
                let duration = Duration.seconds(elapsed).formatted(
                    .units(allowed: [.days, .hours, .minutes], width: .abbreviated, maximumUnitCount: 2)
                )
                parts.append(String(localized: "firing for \(duration)"))
            }
        case .resolved:
            if let resolvedAt = detail.resolvedAt {
                parts.append(String(localized: "resolved \(Formatters.formatRelativeTime(resolvedAt))"))
            }
        }
        return parts.joined(separator: " \u{00B7} ")
    }

    // MARK: - Info rows

    func infoGroup(_ detail: MobileAlertDetail) -> some View {
        VStack(spacing: 0) {
            NavigationLink {
                ServerDetailView(serverId: detail.serverId)
            } label: {
                AlertInfoRow(label: String(localized: "Server"), value: detail.serverName, isLink: true)
            }
            .buttonStyle(.plain)
            rowDivider
            AlertInfoRow(label: String(localized: "First triggered"), value: AlertAbsoluteTime.string(from: detail.firstTriggeredAt))
            rowDivider
            AlertInfoRow(label: String(localized: "Resolved at"), value: detail.resolvedAt.map(AlertAbsoluteTime.string(from:)))
            rowDivider
            AlertInfoRow(label: String(localized: "Trigger count"), value: "\(detail.triggerCount)")
            rowDivider
            AlertInfoRow(label: String(localized: "Trigger mode"), value: triggerModeLabel(detail.ruleTriggerMode))
            rowDivider
            AlertInfoRow(
                label: String(localized: "Rule enabled"),
                value: detail.ruleEnabled ? String(localized: "Yes") : String(localized: "No")
            )
        }
        .cardSurface(padding: 0)
    }

    var rowDivider: some View {
        Divider().padding(.leading, 16)
    }

    /// Humanizes the server's `trigger_mode` (`always` / `once`); unknown
    /// values are shown as sent.
    func triggerModeLabel(_ mode: String) -> String {
        switch mode {
        case "always": String(localized: "Always")
        case "once": String(localized: "Once")
        default: mode
        }
    }

    // MARK: - Message

    func messageGroup(_ message: String) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            GroupHeader(String(localized: "Message"))
            Text(message)
                .font(.subheadline)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .cardSurface()
        }
    }

    // MARK: - View server

    func viewServerButton(serverId: String) -> some View {
        NavigationLink {
            ServerDetailView(serverId: serverId)
        } label: {
            Text(String(localized: "View Server"))
                .font(.headline)
                .foregroundStyle(.white)
                .frame(maxWidth: .infinity, minHeight: buttonHeight)
                .background(Color.accentColor, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                .contentShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        }
        .buttonStyle(.plain)
    }
}

/// Label → value row inside the detail's grouped card. `isLink` tints the value
/// with the accent colour and adds a disclosure chevron.
private struct AlertInfoRow: View {
    let label: String
    let value: String?
    var isLink = false

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text(label)
                .foregroundStyle(.primary)
            Spacer(minLength: 12)
            HStack(spacing: 4) {
                Text(value ?? "\u{2014}")
                    .monospacedDigit()
                    .multilineTextAlignment(.trailing)
                    .foregroundStyle(isLink ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(.secondary))
                if isLink {
                    Image(systemName: "chevron.right")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(.tertiary)
                        .accessibilityHidden(true)
                }
            }
        }
        .font(.subheadline)
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
        .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(label))
        .accessibilityValue(Text(value ?? String(localized: "Not available")))
    }
}

/// Absolute timestamp with relative day names: "Today, 14:20" / "今天 14:20".
private enum AlertAbsoluteTime {
    private static let formatter: DateFormatter = {
        let f = DateFormatter()
        f.dateStyle = .medium
        f.timeStyle = .short
        f.doesRelativeDateFormatting = true
        return f
    }()

    static func string(from isoString: String) -> String {
        guard let date = ISO8601DateFormatter.shared.date(from: isoString) else { return isoString }
        return formatter.string(from: date)
    }
}
