import SwiftUI

/// Tinted "Firing" / "Resolved" capsule. Firing uses the alert colour; green is
/// reserved for the resolved (healthy) state.
struct AlertStatusBadge: View {
    let status: AlertStatus

    private var label: String {
        status == .firing
            ? String(localized: "Firing")
            : String(localized: "Resolved")
    }

    private var color: Color {
        status == .firing ? .alertFiring : .serverOnline
    }

    var body: some View {
        StatusBadge(text: label, color: color)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(Text(String(localized: "Alert status")))
            .accessibilityValue(Text(label))
    }
}

/// Muted gray capsule for secondary tags shown beside an alert status, such as
/// the trigger count ("×3") or a rule's "All conditions" mode.
struct AlertTagCapsule: View {
    let text: String
    var weight: Font.Weight = .regular

    var body: some View {
        Text(text)
            .font(.caption.weight(weight))
            .monospacedDigit()
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .padding(.horizontal, 7)
            .padding(.vertical, 1)
            .background(Color(.tertiarySystemFill), in: Capsule())
    }
}

extension AlertTagCapsule {
    /// "×N" trigger-count tag. Only meaningful when `count > 1`.
    static func count(_ count: Int) -> AlertTagCapsule {
        AlertTagCapsule(text: "\u{00D7}\(count)")
    }
}
