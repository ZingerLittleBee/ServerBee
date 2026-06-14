import SwiftUI

/// A titled card container used throughout the detail screens. Wraps content in
/// the standard rounded-rectangle surface with an optional header and trailing
/// accessory (e.g. a "view all" affordance).
struct SectionCard<Content: View, Accessory: View>: View {
    let title: String?
    var systemImage: String?
    @ViewBuilder var content: Content
    @ViewBuilder var accessory: Accessory

    init(
        _ title: String? = nil,
        systemImage: String? = nil,
        @ViewBuilder content: () -> Content,
        @ViewBuilder accessory: () -> Accessory = { EmptyView() }
    ) {
        self.title = title
        self.systemImage = systemImage
        self.content = content()
        self.accessory = accessory()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let title {
                HStack {
                    Label {
                        Text(title)
                    } icon: {
                        if let systemImage {
                            Image(systemName: systemImage)
                        }
                    }
                    .font(.headline)
                    .labelStyle(TitleIconLabelStyle())
                    Spacer()
                    accessory
                }
            }
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 14))
        .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
    }
}

/// Label style that only shows the icon when one is provided (avoids a blank
/// leading gap for icon-less section titles).
private struct TitleIconLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 6) {
            configuration.icon
                .foregroundStyle(.secondary)
            configuration.title
        }
    }
}

/// A single label → value row used in metadata cards.
struct DetailRow: View {
    let label: String
    let value: String?
    var systemImage: String?
    var valueColor: Color = .primary
    var monospaced = false

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            if let systemImage {
                Image(systemName: systemImage)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(width: 18)
                    .accessibilityHidden(true)
            }
            Text(label)
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Spacer(minLength: 12)
            Text(value ?? "—")
                .font(monospaced ? .subheadline.monospaced() : .subheadline)
                .foregroundStyle(value == nil ? AnyShapeStyle(.tertiary) : AnyShapeStyle(valueColor))
                .multilineTextAlignment(.trailing)
                .textSelection(.enabled)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(label))
        .accessibilityValue(Text(value ?? String(localized: "Not available")))
    }
}

/// Horizontally-scrollable pill selector. Scales past the ~4-item limit of a
/// native segmented control, so a server with many capability sections still
/// gets a tidy, swipeable section bar.
struct SegmentedScrollBar<Tab: Hashable & Identifiable>: View {
    let tabs: [Tab]
    @Binding var selection: Tab
    let title: (Tab) -> String

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(tabs) { tab in
                        let isSelected = tab == selection
                        Button {
                            withAnimation(.easeInOut(duration: 0.15)) { selection = tab }
                        } label: {
                            Text(title(tab))
                                .font(.subheadline.weight(isSelected ? .semibold : .regular))
                                .foregroundStyle(isSelected ? Color.white : Color.primary)
                                .padding(.horizontal, 14)
                                .padding(.vertical, 7)
                                .background(isSelected ? Color.accentColor : Color(.systemGray5))
                                .clipShape(Capsule())
                        }
                        .buttonStyle(.plain)
                        .id(tab.id)
                    }
                }
                .padding(.horizontal, 2)
            }
            .onChange(of: selection) { _, newValue in
                withAnimation { proxy.scrollTo(newValue.id, anchor: .center) }
            }
        }
    }
}

/// Online/offline status badge with a colored dot.
struct StatusPill: View {
    let isOnline: Bool

    var body: some View {
        let label = isOnline ? String(localized: "Online") : String(localized: "Offline")
        let color = isOnline ? Color.serverOnline : Color.serverOffline
        HStack(spacing: 6) {
            Circle()
                .fill(color)
                .frame(width: 9, height: 9)
                .accessibilityHidden(true)
            Text(label)
                .font(.subheadline.bold())
                .foregroundStyle(color)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 5)
        .background(color.opacity(0.12))
        .clipShape(Capsule())
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(String(localized: "Status")))
        .accessibilityValue(Text(label))
    }
}

/// A small rounded tag/chip (capabilities, monitor types, tags, …).
struct Chip: View {
    let text: String
    var systemImage: String?
    var color: Color = .secondary
    var filled = true

    var body: some View {
        HStack(spacing: 4) {
            if let systemImage {
                Image(systemName: systemImage)
                    .font(.caption2)
                    .accessibilityHidden(true)
            }
            Text(text)
                .font(.caption2.weight(.medium))
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .foregroundStyle(filled ? color : .secondary)
        .background(filled ? color.opacity(0.12) : Color(.systemGray6))
        .clipShape(Capsule())
    }
}

/// A horizontal usage/progress bar with a value-driven fill colour.
///
/// `value` is a fraction in `0...` (values > 1 clamp the fill but the caller is
/// expected to surface the overage in an adjacent label). Colour shifts from
/// green → amber → red as usage climbs, matching the web traffic bar thresholds.
struct UsageBar: View {
    let value: Double
    var height: CGFloat = 10
    var tint: Color?

    private var clamped: Double { min(max(value, 0), 1) }

    private var fillColor: Color {
        if let tint { return tint }
        switch value {
        case ..<0.7: return .serverOnline
        case ..<0.9: return .warningAmber
        default: return .serverOffline
        }
    }

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color(.systemGray5))
                Capsule()
                    .fill(fillColor)
                    .frame(width: geo.size.width * clamped)
            }
        }
        .frame(height: height)
        .accessibilityElement()
        .accessibilityValue(Text(String(format: "%.0f%%", value * 100)))
    }
}

/// Wraps chips into rows that flow onto multiple lines.
struct FlowChips<Item: Hashable, ChipView: View>: View {
    let items: [Item]
    @ViewBuilder let chip: (Item) -> ChipView

    var body: some View {
        FlexibleWrap(items: items, spacing: 6, lineSpacing: 6) { item in
            chip(item)
        }
    }
}

/// Minimal flow layout that wraps subviews onto new lines when they exceed the
/// available width. Uses SwiftUI's `Layout` so it adapts to Dynamic Type.
struct FlexibleWrap<Item: Hashable, ItemView: View>: View {
    let items: [Item]
    var spacing: CGFloat = 6
    var lineSpacing: CGFloat = 6
    @ViewBuilder let content: (Item) -> ItemView

    var body: some View {
        WrapLayout(spacing: spacing, lineSpacing: lineSpacing) {
            ForEach(items, id: \.self) { item in
                content(item)
            }
        }
    }
}

struct WrapLayout: Layout {
    var spacing: CGFloat = 6
    var lineSpacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let maxWidth = proposal.width ?? .infinity
        var rows: [[CGSize]] = [[]]
        var rowWidth: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if rowWidth + size.width > maxWidth, !(rows.last?.isEmpty ?? true) {
                rows.append([])
                rowWidth = 0
            }
            rows[rows.count - 1].append(size)
            rowWidth += size.width + spacing
        }
        let height = rows.reduce(0) { acc, row in
            acc + (row.map(\.height).max() ?? 0) + lineSpacing
        } - (rows.isEmpty ? 0 : lineSpacing)
        return CGSize(width: maxWidth == .infinity ? rowWidth : maxWidth, height: max(0, height))
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX
        var y = bounds.minY
        var lineHeight: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x + size.width > bounds.maxX, x > bounds.minX {
                x = bounds.minX
                y += lineHeight + lineSpacing
                lineHeight = 0
            }
            subview.place(at: CGPoint(x: x, y: y), anchor: .topLeading, proposal: ProposedViewSize(size))
            x += size.width + spacing
            lineHeight = max(lineHeight, size.height)
        }
    }
}
