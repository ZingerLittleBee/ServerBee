import SwiftUI

// MARK: - Summary

/// Event-type KPI summary (brute force / port scan / login counts).
struct SecuritySummaryCard: View {
    let typeCounts: [StatsBucket]

    private func count(_ type: String) -> Int {
        typeCounts.first { $0.key == type }?.count ?? 0
    }

    var body: some View {
        SectionCard(String(localized: "Last 30 days"), systemImage: "shield.lefthalf.filled") {
            HStack(spacing: 12) {
                kpi(count("ssh_brute_force"), String(localized: "Brute force"), "lock.trianglebadge.exclamationmark", .red)
                kpi(count("port_scan"), String(localized: "Port scans"), "dot.radiowaves.left.and.right", .orange)
                kpi(count("ssh_login"), String(localized: "Logins"), "person.badge.key", .blue)
            }
        }
    }

    private func kpi(_ value: Int, _ label: String, _ icon: String, _ color: Color) -> some View {
        VStack(spacing: 4) {
            Image(systemName: icon)
                .font(.title3)
                .foregroundStyle(color)
            Text("\(value)")
                .font(.title2.bold().monospacedDigit())
            Text(label)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
    }
}

// MARK: - Feed

/// Scrollable list of security events with a load-more affordance.
struct SecurityFeedCard: View {
    let events: [SecurityEvent]
    let onSelect: (SecurityEvent) -> Void
    let canLoadMore: Bool
    let isLoadingMore: Bool
    let onLoadMore: () -> Void

    var body: some View {
        SectionCard(String(localized: "Events"), systemImage: "list.bullet.rectangle") {
            VStack(spacing: 0) {
                ForEach(events) { event in
                    Button { onSelect(event) } label: {
                        SecurityEventRow(event: event)
                    }
                    .buttonStyle(.plain)
                    if event.id != events.last?.id {
                        Divider()
                    }
                }
                if canLoadMore {
                    Divider()
                    Button(action: onLoadMore) {
                        HStack {
                            if isLoadingMore {
                                ProgressView().controlSize(.small)
                            }
                            Text(isLoadingMore ? String(localized: "Loading…") : String(localized: "Load more"))
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 8)
                    }
                    .disabled(isLoadingMore)
                }
            }
        }
    }
}

/// One event row: severity-tinted type icon, type/IP/time, first-seen badge.
struct SecurityEventRow: View {
    let event: SecurityEvent

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: SecurityEventKind.icon(event.eventType))
                .font(.body)
                .foregroundStyle(SecurityEventKind.color(event.eventType))
                .frame(width: 24)

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(SecurityEventKind.label(event.eventType))
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.primary)
                    SeverityBadge(severity: event.severity)
                    if event.firstSeen {
                        Chip(text: String(localized: "New"), color: .blue)
                    }
                }
                HStack(spacing: 6) {
                    Text(event.sourceIp)
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                    if let user = event.username {
                        Text("· \(user)")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
                if let summary = event.evidence?.summary {
                    Text(summary)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                if let date = event.date {
                    Text(date, style: .time)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                    Text(date, format: .dateTime.month().day())
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }
        }
        .padding(.vertical, 8)
        .contentShape(Rectangle())
    }
}

/// Small severity capsule.
struct SeverityBadge: View {
    let severity: String

    var body: some View {
        let color = SecuritySeverity.color(severity)
        Text(SecuritySeverity.label(severity))
            .font(.caption2.weight(.semibold))
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .foregroundStyle(color)
            .background(color.opacity(0.14))
            .clipShape(Capsule())
    }
}
