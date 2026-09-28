import SwiftUI

// Small grouped-surface building blocks for the server Overview tab.

/// One label → value line in an Overview grouped card.
struct OverviewInfoRow: Identifiable {
    var id: String { label }
    let label: String
    let value: String
    var valueColor: Color?
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

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                if index > 0 {
                    Divider().padding(.leading, 16)
                }
                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    // Priorities keep the short label whole and let the value take the
                    // rest of the row before the spacer, instead of an even split.
                    Text(row.label)
                        .foregroundStyle(.primary)
                        .layoutPriority(2)
                    Spacer(minLength: 12)
                    Text(row.value)
                        .foregroundStyle(row.valueColor ?? .secondary)
                        .monospacedDigit()
                        .multilineTextAlignment(.trailing)
                        .textSelection(.enabled)
                        .layoutPriority(1)
                }
                .font(.subheadline)
                .padding(.horizontal, 16)
                .padding(.vertical, 11)
                .accessibilityElement(children: .combine)
            }
        }
        .cardSurface(padding: 0)
    }
}
