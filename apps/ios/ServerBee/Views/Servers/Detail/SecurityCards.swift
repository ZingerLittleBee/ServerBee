import SwiftUI

// MARK: - Summary

/// Event-type KPI summary: large colored brute force / port scan / login counts
/// under a "Last 30 days" title with the total across every event type.
struct SecuritySummaryCard: View {
    let typeCounts: [StatsBucket]

    private func count(_ type: String) -> Int {
        typeCounts.first { $0.key == type }?.count ?? 0
    }

    /// Sum of every bucket, including event types without a dedicated KPI.
    private var total: Int {
        typeCounts.reduce(0) { $0 + $1.count }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(String(localized: "Last 30 days"))
                    .font(.subheadline.weight(.semibold))
                    .accessibilityAddTraits(.isHeader)
                Spacer(minLength: 8)
                Text(total == 1 ? String(localized: "1 event") : String(localized: "\(total) events"))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            HStack(alignment: .top, spacing: 8) {
                kpi("ssh_brute_force", String(localized: "Brute force"))
                kpi("port_scan", String(localized: "Port scans"))
                kpi("ssh_login", String(localized: "Logins"))
            }
        }
        .cardSurface()
    }

    private func kpi(_ type: String, _ label: String) -> some View {
        let value = count(type)
        return VStack(alignment: .leading, spacing: 2) {
            Text(value, format: .number)
                .font(.title.bold())
                .monospacedDigit()
                .foregroundStyle(SecurityEventKind.color(type))
                .lineLimit(1)
                .minimumScaleFactor(0.6)
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(2)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(label))
        .accessibilityValue(Text(value, format: .number))
    }
}

// MARK: - Feed

/// "Events" group: a grouped surface of event rows plus a plain load-more
/// affordance underneath.
struct SecurityFeedCard: View {
    let events: [SecurityEvent]
    let onSelect: (SecurityEvent) -> Void
    let canLoadMore: Bool
    let isLoadingMore: Bool
    let onLoadMore: () -> Void
    /// Optional resolver showing which server an event came from — used in the
    /// fleet-wide overview where events span multiple servers.
    var serverName: (SecurityEvent) -> String? = { _ in nil }

    var body: some View {
        VStack(spacing: 8) {
            GroupHeader(String(localized: "Events"))
            SecurityEventRows(events: events, onSelect: onSelect, serverName: serverName)
            if canLoadMore {
                loadMoreButton
            }
        }
    }

    private var loadMoreButton: some View {
        Button(action: onLoadMore) {
            HStack(spacing: 6) {
                if isLoadingMore {
                    ProgressView().controlSize(.small)
                }
                Text(isLoadingMore ? String(localized: "Loading…") : String(localized: "Load more"))
            }
            .font(.subheadline)
            .frame(maxWidth: .infinity, minHeight: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.borderless)
        .disabled(isLoadingMore)
    }
}

/// The grouped surface holding the event rows, separated by hairlines inset to
/// the text column (past the icon tile).
private struct SecurityEventRows: View {
    let events: [SecurityEvent]
    let onSelect: (SecurityEvent) -> Void
    let serverName: (SecurityEvent) -> String?

    @ScaledMetric(relativeTo: .body) private var iconSize: CGFloat = SecurityKindIcon.defaultSize

    var body: some View {
        VStack(spacing: 0) {
            ForEach(events) { event in
                if event.id != events.first?.id {
                    Divider()
                        .padding(.leading, SecurityEventRow.horizontalPadding + iconSize + SecurityEventRow.iconSpacing)
                }
                Button { onSelect(event) } label: {
                    SecurityEventRow(event: event, serverName: serverName(event))
                }
                .buttonStyle(SecurityRowButtonStyle())
            }
        }
        .cardSurface(padding: 0)
    }
}

/// Grouped-row press feedback: a subtle fill while the finger is down.
private struct SecurityRowButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(configuration.isPressed ? Color(.systemGray5) : Color.clear)
    }
}

/// One event row: tinted kind tile, title + severity + first-seen badges,
/// source IP · username, evidence summary, and a trailing time/date column.
/// At accessibility text sizes the time/date moves under the details so the
/// row never overflows horizontally.
/// `serverName`, when set, adds a server line (used in the fleet overview).
struct SecurityEventRow: View {
    let event: SecurityEvent
    var serverName: String?

    static let horizontalPadding: CGFloat = 16
    static let iconSpacing: CGFloat = 12

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.privacyMode) private var privacyMode

    var body: some View {
        HStack(alignment: .top, spacing: Self.iconSpacing) {
            SecurityKindIcon(eventType: event.eventType)
            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 4) {
                    details
                    timestamp
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    details
                    Spacer(minLength: 0)
                    timestamp
                        .fixedSize()
                }
            }
        }
        .padding(.horizontal, Self.horizontalPadding)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
    }

    private var details: some View {
        VStack(alignment: .leading, spacing: 4) {
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 6) {
                    title
                    badges
                }
                VStack(alignment: .leading, spacing: 4) {
                    title
                    HStack(spacing: 6) { badges }
                }
            }
            if let serverName {
                Label(serverName, systemImage: "server.rack")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Text(verbatim: sourceLine)
                .font(.footnote)
                .monospacedDigit()
                .foregroundStyle(.primary)
            if let summary = event.evidence?.summary {
                Text(summary)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var title: some View {
        Text(SecurityEventKind.label(event.eventType))
            .font(.callout.weight(.semibold))
            .foregroundStyle(.primary)
            .fixedSize(horizontal: false, vertical: true)
    }

    @ViewBuilder
    private var badges: some View {
        SeverityBadge(severity: event.severity)
            .fixedSize()
        if event.firstSeen {
            StatusBadge(text: String(localized: "New"), color: .accentColor)
                .fixedSize()
        }
    }

    /// "203.0.113.5 · deploy" — the username is appended when known. The
    /// no-break space keeps the separator on the IP's line when a long
    /// (IPv6) address forces a wrap, so "· user" never starts a line.
    private var sourceLine: String {
        let ip = event.sourceIp.maskingIPs(privacyMode)
        guard let user = event.username, !user.isEmpty else { return ip }
        return "\(ip)\u{00A0}· \(user)"
    }

    /// Time over date in the trailing column; side by side when inlined at
    /// accessibility sizes.
    @ViewBuilder
    private var timestamp: some View {
        if let date = event.date {
            let layout = dynamicTypeSize.isAccessibilitySize
                ? AnyLayout(HStackLayout(alignment: .firstTextBaseline, spacing: 6))
                : AnyLayout(VStackLayout(alignment: .trailing, spacing: 2))
            layout {
                Text(date, style: .time)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                Text(date, format: .dateTime.month().day())
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            .monospacedDigit()
            .lineLimit(1)
        }
    }
}

// MARK: - Badges & icon

/// Severity capsule, tinted by `SecuritySeverity.color`.
struct SeverityBadge: View {
    let severity: String

    var body: some View {
        StatusBadge(text: SecuritySeverity.label(severity), color: SecuritySeverity.color(severity))
    }
}

/// Rounded square tinted with the event kind's colour at low opacity, holding
/// the kind's coloured SF Symbol. Scales with Dynamic Type.
struct SecurityKindIcon: View {
    static let defaultSize: CGFloat = 30

    let eventType: String
    @ScaledMetric private var size: CGFloat

    init(eventType: String, baseSize: CGFloat = SecurityKindIcon.defaultSize) {
        self.eventType = eventType
        _size = ScaledMetric(wrappedValue: baseSize, relativeTo: .body)
    }

    var body: some View {
        let color = SecurityEventKind.color(eventType)
        Image(systemName: SecurityEventKind.icon(eventType))
            .font(.system(size: size * 0.55, weight: .semibold))
            .foregroundStyle(color)
            .frame(width: size, height: size)
            .background(color.opacity(0.14), in: RoundedRectangle(cornerRadius: size * 0.27, style: .continuous))
            .accessibilityHidden(true)
    }
}
