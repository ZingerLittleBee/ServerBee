import SwiftUI

/// A compact card displaying a single metric with label, value, and optional subtitle.
/// Used in the 2-column metrics grid on the server detail view.
struct MetricCardView: View {
    let label: String
    let value: String
    var subtitle: String?
    var valueColor: Color = .primary

    @ScaledMetric(relativeTo: .body) private var verticalPad: CGFloat = 14
    @ScaledMetric(relativeTo: .body) private var horizontalPad: CGFloat = 14

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.title2.bold())
                .foregroundStyle(valueColor)
                .minimumScaleFactor(0.7)
                .lineLimit(1)
            if let subtitle {
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, horizontalPad)
        .padding(.vertical, verticalPad)
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(label))
        .accessibilityValue(Text(subtitle.map { "\(value), \($0)" } ?? value))
    }
}

#Preview {
    VStack(spacing: 12) {
        MetricCardView(
            label: "CPU",
            value: "45.2%",
            subtitle: "Intel i7-12700K",
            valueColor: .green
        )
        MetricCardView(
            label: "Memory",
            value: "72.3%",
            subtitle: "11.6 GB / 16.0 GB",
            valueColor: .orange
        )
    }
    .padding()
    .background(Color(.systemGroupedBackground))
}
