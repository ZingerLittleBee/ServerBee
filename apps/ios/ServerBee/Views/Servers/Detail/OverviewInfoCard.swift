import SwiftUI
import UIKit

// Small grouped-surface building blocks for the server Overview tab.

/// One label → value line in an Overview grouped card.
struct OverviewInfoRow: Identifiable {
    var id: String { label }
    let label: String
    let value: String
    var copyValue: String?
    var valueColor: Color?

    static func optional(label: String, value: String?, copyValue: String? = nil) -> Self? {
        guard let value, !value.isEmpty else { return nil }
        return Self(label: label, value: value, copyValue: copyValue)
    }
}

/// A `GroupHeader` above its card, matching an inset-grouped list section.
struct OverviewInfoGroup<Content: View>: View {
    let title: String
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            GroupHeader(title)
            content
        }
    }
}

/// Settings-style inset rows (label leading, secondary value trailing) with
/// hairline separators, on a card surface.
struct OverviewInfoCard: View {
    let rows: [OverviewInfoRow]
    @State private var copiedRowId: String?

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                if index > 0 {
                    Divider().padding(.leading, 16)
                }
                rowView(row)
            }
        }
        .cardSurface(padding: 0)
    }

    @ViewBuilder
    private func rowView(_ row: OverviewInfoRow) -> some View {
        if let copyValue = row.copyValue {
            Button {
                copyToClipboard(copyValue, rowId: row.id)
            } label: {
                rowContent(row, isCopyable: true)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .contextMenu {
                Button {
                    copyToClipboard(copyValue, rowId: row.id)
                } label: {
                    Label(String(localized: "Copy"), systemImage: "doc.on.doc")
                }
            }
        } else {
            rowContent(row, isCopyable: false)
        }
    }

    private func rowContent(_ row: OverviewInfoRow, isCopyable: Bool) -> some View {
        let didCopy = copiedRowId == row.id
        return HStack(alignment: .firstTextBaseline, spacing: 12) {
            // Priorities keep the short label whole and let the value take the
            // rest of the row before the spacer, instead of an even split.
            Text(row.label)
                .foregroundStyle(.primary)
                .layoutPriority(2)
            Spacer(minLength: 12)
            if didCopy {
                Label(String(localized: "Copied"), systemImage: "checkmark")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(Color.serverOnline)
            } else if isCopyable {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(row.value)
                        .foregroundStyle(row.valueColor ?? .secondary)
                        .monospacedDigit()
                        .multilineTextAlignment(.trailing)
                    Image(systemName: "doc.on.doc")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .accessibilityHidden(true)
                }
                .layoutPriority(1)
            } else {
                Text(row.value)
                    .foregroundStyle(row.valueColor ?? .secondary)
                    .monospacedDigit()
                    .multilineTextAlignment(.trailing)
                    .textSelection(.enabled)
                    .layoutPriority(1)
            }
        }
        .font(.subheadline)
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
        .accessibilityElement(children: .combine)
    }

    private func copyToClipboard(_ value: String, rowId: String) {
        UIPasteboard.general.string = value
        copiedRowId = rowId
        Task {
            try? await Task.sleep(for: .seconds(1.5))
            if copiedRowId == rowId {
                copiedRowId = nil
            }
        }
    }
}

/// Agent capability chips card shown on `ServerOverviewSection`.
struct ServerCapabilitiesCard: View {
    let capabilities: CapabilitySet

    var body: some View {
        let enabled = Capability.allCases.filter { capabilities.isEnabled($0) }
        let disabled = Capability.allCases.filter { !capabilities.isEnabled($0) }
        let gaps = capabilities.configuredButUnavailable
        let isKnown = capabilities.configured != nil || capabilities.agentLocal != nil || capabilities.effective != nil
        VStack(alignment: .leading, spacing: 10) {
            if enabled.isEmpty {
                Text(String(localized: "No capabilities enabled."))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            if isKnown {
                FlexibleWrap(items: enabled + disabled) { cap in
                    if enabled.contains(cap) {
                        Chip(text: cap.label, color: .accentColor)
                    } else {
                        Chip(text: String(localized: "\(cap.label) · off"), color: .secondary)
                    }
                }
            }
            if !gaps.isEmpty {
                Label {
                    Text(String(
                        format: String(localized: "Configured but unavailable: %@"),
                        gaps.map(\.label).joined(separator: ", ")
                    ))
                } icon: {
                    Image(systemName: "exclamationmark.triangle")
                        .foregroundStyle(Color.warningAmber)
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            Text(String(localized: "Set on the agent host. Read-only here."))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .cardSurface()
    }
}
