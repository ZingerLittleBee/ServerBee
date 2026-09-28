import SwiftUI

/// A metric tile for the server Overview grid: a small label above arbitrary
/// metric content (a `MetricTileReadout`, a `UsageBar`, a caption, …) on the
/// standard card surface. The tile stretches to its grid row's height so tiles
/// side by side line up, and reads to VoiceOver as one "label, value" element.
struct MetricCardView<Content: View>: View {
    let label: String
    let accessibilityValue: String
    @ViewBuilder var content: Content

    init(_ label: String, accessibilityValue: String, @ViewBuilder content: () -> Content) {
        self.label = label
        self.accessibilityValue = accessibilityValue
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label)
                .font(.footnote.weight(.medium))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            content
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .cardSurface()
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(label))
        .accessibilityValue(Text(accessibilityValue))
    }
}

/// Large tinted metric number with a smaller trailing unit in the same tint
/// (`23` `%`, `3.1` `/ 8 GB`). Metric colours are pastel, so in light mode the
/// tint is deepened slightly to keep the number legible on a white card.
struct MetricTileReadout: View {
    let value: String
    var unit: String?
    var color: Color = .primary
    var font: Font = .title.weight(.bold)
    var unitFont: Font = .callout.weight(.bold)
    var systemImage: String?

    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 2) {
            if let systemImage {
                Image(systemName: systemImage)
                    .font(unitFont)
                    .accessibilityHidden(true)
            }
            Text(value)
                .font(font)
            if let unit {
                Text(unit)
                    .font(unitFont)
            }
        }
        .foregroundStyle(color)
        .brightness(colorScheme == .dark ? 0 : -0.12)
        .monospacedDigit()
        .lineLimit(1)
        .minimumScaleFactor(0.6)
    }
}

#Preview {
    Grid(horizontalSpacing: 10, verticalSpacing: 10) {
        GridRow {
            MetricCardView("CPU", accessibilityValue: "23.4%") {
                MetricTileReadout(value: "23.4", unit: "%", color: .cpuColor)
                UsageBar(value: 0.234, height: 6, tint: .cpuColor)
            }
            MetricCardView("Network", accessibilityValue: "↓ 4.8 MB/s, ↑ 1.2 MB/s") {
                MetricTileReadout(value: "4.8 MB/s", color: .networkColor, font: .title3.weight(.bold), systemImage: "arrow.down")
                MetricTileReadout(value: "1.2 MB/s", font: .title3.weight(.bold), systemImage: "arrow.up")
            }
        }
    }
    .padding()
    .background(Color(.systemGroupedBackground))
}
