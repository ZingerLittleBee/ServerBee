import SwiftUI

// Shared building blocks for the "Native Refined" visual language: grouped
// surfaces, compact stat tiles, status capsules, icon tiles and inline usage
// bars. Screens compose these instead of restyling ad hoc so light/dark and
// Dynamic Type stay consistent.

// MARK: - Surface

extension View {
    /// Card surface used on grouped screens: secondary grouped background,
    /// 16pt continuous corners, no shadow.
    func cardSurface(padding: CGFloat = 14) -> some View {
        self
            .padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color(.secondarySystemGroupedBackground))
            .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
    }
}

/// Header placed above a card or group on ScrollView-based screens, matching
/// the inset-grouped `List` section header of the running OS: sentence-case
/// headline on iOS 26, uppercase footnote before it.
struct GroupHeader: View {
    let title: String

    init(_ title: String) {
        self.title = title
    }

    var body: some View {
        styledTitle
            .foregroundStyle(.secondary)
            .padding(.horizontal, 16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityAddTraits(.isHeader)
    }

    @ViewBuilder private var styledTitle: some View {
        if #available(iOS 26, *) {
            Text(title).font(.headline)
        } else {
            Text(title).font(.footnote).textCase(.uppercase)
        }
    }
}

// MARK: - Stat tile

/// A compact label-over-value tile (fleet summary, Docker resources, …).
struct StatTile<Value: View>: View {
    let label: String
    @ViewBuilder var value: Value

    init(_ label: String, @ViewBuilder value: () -> Value) {
        self.label = label
        self.value = value()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(.caption.weight(.medium))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            value
        }
        .cardSurface(padding: 12)
        .accessibilityElement(children: .combine)
    }
}

/// Large numeric value used inside a `StatTile`, with an optional muted suffix
/// (e.g. `5` + `/ 6`, `38` + `MB/s`).
struct StatValue: View {
    let value: String
    var suffix: String?
    var color: Color = .primary

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 3) {
            Text(value)
                .font(.title2.weight(.bold))
                .foregroundStyle(color)
            if let suffix {
                Text(suffix)
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.tertiary)
            }
        }
        .monospacedDigit()
        .lineLimit(1)
        .minimumScaleFactor(0.7)
    }
}

// MARK: - Status badge

/// Small tinted capsule for a state label ("Firing", "Resolved", "Running",
/// "Unlocked", severity, …). Background is the tint at low opacity.
struct StatusBadge: View {
    let text: String
    let color: Color
    var systemImage: String?

    var body: some View {
        HStack(spacing: 3) {
            if let systemImage {
                Image(systemName: systemImage)
                    .font(.caption2.weight(.bold))
                    .accessibilityHidden(true)
            }
            Text(text)
                .font(.caption.weight(.bold))
        }
        .foregroundStyle(color)
        .padding(.horizontal, 8)
        .padding(.vertical, 2)
        .background(color.opacity(0.15), in: Capsule())
    }
}

// MARK: - Icon tile

/// Settings-style rounded square holding a white SF Symbol.
struct IconTile: View {
    let systemImage: String
    let color: Color
    @ScaledMetric(relativeTo: .body) private var scaledSize: CGFloat = 30

    /// Capped so accessibility text sizes keep the row's width for the title.
    private var size: CGFloat { min(scaledSize, 44) }

    var body: some View {
        Image(systemName: systemImage)
            .font(.system(size: size * 0.55, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(color, in: RoundedRectangle(cornerRadius: size * 0.27, style: .continuous))
            .accessibilityHidden(true)
    }
}

/// A navigation row with a leading icon tile, title, optional subtitle and an
/// optional trailing value. Use inside a `List` row or a `NavigationLink` label.
/// At accessibility text sizes the value moves under the title so the title
/// keeps the full row width.
struct IconRowLabel: View {
    let title: String
    let systemImage: String
    let color: Color
    var subtitle: String?
    var value: String?

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        HStack(spacing: 12) {
            IconTile(systemImage: systemImage, color: color)
            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 2) {
                    titleStack
                    if let value { valueText(value) }
                }
                Spacer(minLength: 0)
            } else {
                titleStack
                Spacer(minLength: 8)
                if let value { valueText(value) }
            }
        }
        .accessibilityElement(children: .combine)
    }

    private var titleStack: some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(title)
                .foregroundStyle(.primary)
            if let subtitle {
                Text(subtitle)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 1)
            }
        }
    }

    private func valueText(_ value: String) -> some View {
        Text(value)
            .foregroundStyle(.secondary)
            .monospacedDigit()
            .lineLimit(1)
    }
}

// MARK: - Inline usage bar

/// Labelled thin usage bar ("CPU 23%") used in dense rows. Switches to the
/// warning colour at or above `warnAt` percent.
struct InlineUsageBar: View {
    let label: String
    /// Percentage in `0...100`; `nil` renders a dash and an empty track.
    let percent: Double?
    let color: Color
    var warnAt: Double = 85

    private var isHot: Bool { (percent ?? 0) >= warnAt }
    private var fill: Color { isHot ? .warningAmber : color }

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(verbatim: "\(label) \(percent.map { String(format: "%.0f%%", $0) } ?? "—")")
                .font(.caption2.weight(isHot ? .semibold : .regular))
                .foregroundStyle(isHot ? Color.warningAmber : .secondary)
                .monospacedDigit()
                .lineLimit(1)
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color(.systemGray5))
                    Capsule()
                        .fill(fill)
                        .frame(width: geo.size.width * min(max((percent ?? 0) / 100, 0), 1))
                }
            }
            .frame(height: 4)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(label))
        .accessibilityValue(Text(percent.map { String(format: "%.0f%%", $0) } ?? String(localized: "Not available")))
    }
}
