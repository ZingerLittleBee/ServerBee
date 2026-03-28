import SwiftUI

/// A compact card displaying a single metric with label, value, and optional subtitle.
/// Used in the 2-column metrics grid on the server detail view.
struct MetricCardView: View {
    let label: String
    let value: String
    var subtitle: String?
    var valueColor: Color = .primary

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.title3.bold())
                .foregroundStyle(valueColor)
            if let subtitle {
                Text(subtitle)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
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
