import SwiftUI

/// Shared brand and typography for sign-in and session recovery.
struct AuthenticationHeader: View {
    let title: LocalizedStringKey
    let subtitle: LocalizedStringKey
    @ScaledMetric(relativeTo: .largeTitle) private var logoSize: CGFloat = 88
    @Environment(\.colorScheme) private var colorScheme
    private let maximumLogoSize: CGFloat

    init(title: LocalizedStringKey, subtitle: LocalizedStringKey, compact: Bool = false) {
        self.title = title
        self.subtitle = subtitle
        _logoSize = ScaledMetric(wrappedValue: compact ? 64 : 88, relativeTo: .largeTitle)
        maximumLogoSize = compact ? 96 : 132
    }

    var body: some View {
        VStack(spacing: 14) {
            Image("Logo")
                .resizable()
                .scaledToFit()
                .frame(width: min(logoSize, maximumLogoSize), height: min(logoSize, maximumLogoSize))
                .overlay(Circle().strokeBorder((colorScheme == .dark ? Color.white : .black).opacity(0.1), lineWidth: 0.5))
                .shadow(color: .black.opacity(0.10), radius: 15, y: 10)
                .accessibilityHidden(true)
            VStack(spacing: 6) {
                Text(title)
                    .font(.largeTitle.bold())
                    .accessibilityAddTraits(.isHeader)
                Text(subtitle)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
    }
}

// MARK: - Field Row

/// One row of the grouped credentials card: a fixed-width leading label and a
/// trailing field. At accessibility text sizes the label stacks above the
/// field so neither is truncated. Like a native form row, a tap anywhere in
/// the row (label or padding) focuses the field.
struct AuthenticationFieldRow<FieldContent: View>: View {
    let label: String
    let onRowTap: () -> Void
    @ViewBuilder let field: FieldContent

    @ScaledMetric(relativeTo: .body) private var labelWidth: CGFloat = 92
    @ScaledMetric(relativeTo: .body) private var minRowHeight: CGFloat = 46
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        let stacked = dynamicTypeSize.isAccessibilitySize
        let layout = stacked
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 4))
            : AnyLayout(HStackLayout(spacing: 12))

        layout {
            Text(label)
                .frame(width: stacked ? nil : labelWidth, alignment: .leading)
                // Let label taps fall through to the row's tap target below.
                .allowsHitTesting(false)
                // The field below speaks this label instead.
                .accessibilityHidden(true)
            field
                .frame(maxWidth: .infinity, alignment: .leading)
                // A field with a prompt outside a Form exposes no label of its
                // own, so VoiceOver would only read the placeholder.
                .accessibilityLabel(Text(label))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .frame(minHeight: minRowHeight)
        .background {
            // Sits behind the field, so taps on the field itself still reach
            // the text input; only the label and padding land here.
            Color.clear
                .contentShape(Rectangle())
                .onTapGesture(perform: onRowTap)
                .accessibilityHidden(true)
        }
    }
}

/// Hairline separator inset 16pt from the leading edge, as in a grouped list.
struct AuthenticationFieldDivider: View {
    var body: some View {
        Divider()
            .padding(.leading, 16)
    }
}

// MARK: - Button Style

/// Full-width 50pt (Dynamic Type scaled) button: accent fill with white text
/// when prominent, accent text on a light accent tint otherwise. A busy button
/// keeps full opacity so its spinner stays legible; any other disabled button
/// is dimmed.
struct AuthenticationButtonStyle: ButtonStyle {
    let prominent: Bool
    var isBusy = false

    func makeBody(configuration: Configuration) -> some View {
        AuthenticationButtonBody(configuration: configuration, prominent: prominent, isBusy: isBusy)
    }
}

private struct AuthenticationButtonBody: View {
    let configuration: ButtonStyleConfiguration
    let prominent: Bool
    let isBusy: Bool

    @ScaledMetric(relativeTo: .headline) private var height: CGFloat = 50
    @Environment(\.isEnabled) private var isEnabled
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var opacity: Double {
        if configuration.isPressed { return 0.7 }
        return isEnabled || isBusy ? 1 : 0.6
    }

    private var shape: RoundedRectangle {
        RoundedRectangle(cornerRadius: 14, style: .continuous)
    }

    var body: some View {
        configuration.label
            .font(.headline)
            .foregroundStyle(prominent ? Color.white : Color.accentColor)
            .multilineTextAlignment(.center)
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, minHeight: height)
            .background(prominent ? Color.accentColor : Color.accentColor.opacity(0.15), in: shape)
            .contentShape(shape)
            .opacity(opacity)
            .scaleEffect(configuration.isPressed && !reduceMotion ? 0.96 : 1)
            .animation(reduceMotion ? nil : .easeOut(duration: 0.15), value: configuration.isPressed)
    }
}
