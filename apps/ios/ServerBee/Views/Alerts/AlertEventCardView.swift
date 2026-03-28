import SwiftUI

struct AlertEventCardView: View {
    let event: MobileAlertEvent

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                AlertStatusBadge(status: event.status)

                Spacer()

                Text(Formatters.formatRelativeTime(event.updatedAt))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Text(event.ruleName)
                .font(.subheadline.bold())

            Text(event.serverName)
                .font(.caption)
                .foregroundStyle(.secondary)

            if !event.message.isEmpty {
                Text(event.message)
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .lineLimit(2)
            }

            if event.triggerCount > 1 {
                HStack {
                    Spacer()
                    Text("\u{00D7}\(event.triggerCount)")
                        .font(.caption2)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Color(.systemGray5))
                        .clipShape(Capsule())
                }
            }
        }
        .padding(.vertical, 4)
    }
}
