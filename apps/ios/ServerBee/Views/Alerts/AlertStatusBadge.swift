import SwiftUI

struct AlertStatusBadge: View {
    let status: AlertStatus
    var font: Font = .caption2.bold()
    var horizontalPadding: CGFloat = 8
    var verticalPadding: CGFloat = 3

    var body: some View {
        Text(status == .firing ? String(localized: "FIRING") : String(localized: "RESOLVED"))
            .font(font)
            .padding(.horizontal, horizontalPadding)
            .padding(.vertical, verticalPadding)
            .background(status == .firing ? Color.red : Color.green)
            .foregroundStyle(.white)
            .clipShape(Capsule())
    }
}
