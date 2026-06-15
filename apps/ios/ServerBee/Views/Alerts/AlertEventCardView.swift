import SwiftUI

struct AlertEventCardView: View {
    let event: MobileAlertEvent

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                AlertStatusBadge(status: event.status)

                Spacer()

                Text(Formatters.formatRelativeTime(event.eventAt))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Text(event.ruleName)
                .font(.subheadline.bold())

            Text(event.serverName)
                .font(.caption)
                .foregroundStyle(.secondary)

            if event.count > 1 {
                HStack {
                    Spacer()
                    Text("\u{00D7}\(event.count)")
                        .font(.caption2)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Color(.systemGray5))
                        .clipShape(Capsule())
                }
            }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(accessibilityLabelText))
    }

    private var accessibilityLabelText: String {
        let status = event.status == .firing
            ? String(localized: "Firing")
            : String(localized: "Resolved")
        let relative = Formatters.formatRelativeTime(event.eventAt)
        var parts = [status, event.ruleName, event.serverName, relative]
        if event.count > 1 {
            parts.append(String(format: String(localized: "Triggered %d times"), event.count))
        }
        return parts.joined(separator: ", ")
    }
}
