import SwiftUI

struct AlertStatusBadge: View {
    let status: AlertStatus
    var font: Font = .caption2.bold()
    var horizontalPadding: CGFloat = 8
    var verticalPadding: CGFloat = 3

    private var label: String {
        status == .firing
            ? String(localized: "FIRING")
            : String(localized: "RESOLVED")
    }

    var body: some View {
        Text(label)
            .font(font)
            .padding(.horizontal, horizontalPadding)
            .padding(.vertical, verticalPadding)
            .background(status == .firing ? Color.alertFiring : Color.alertResolved)
            .foregroundStyle(.white)
            .clipShape(Capsule())
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(Text(String(localized: "Alert status")))
            .accessibilityValue(Text(label))
    }
}
