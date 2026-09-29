import SwiftUI

/// Fleet summary above the servers list: a 3-up row of stat tiles for
/// online / total servers, firing alerts, and the aggregate live download
/// rate across online servers. Stacks vertically at accessibility sizes.
struct ServerListHeaderView: View {
    let onlineCount: Int
    let totalCount: Int
    /// Number of firing alerts, or `nil` while alert events are unavailable.
    let firingAlertCount: Int?
    /// Sum of live download rates (bytes/s) across online servers.
    let downloadBytesPerSec: Int64

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        let layout = dynamicTypeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(spacing: 10))
            : AnyLayout(HStackLayout(alignment: .top, spacing: 10))
        layout {
            onlineTile
            alertsTile
            trafficTile
        }
    }
}

private extension ServerListHeaderView {
    var onlineTile: some View {
        // Green signals a healthy fleet only; "0 / N" stays neutral like the alerts tile.
        StatTile(String(localized: "Online")) {
            StatValue(
                value: onlineCount.formatted(),
                suffix: "/ \(totalCount.formatted())",
                color: onlineCount > 0 ? .serverOnline : .primary
            )
        }
        .accessibilityLabel(Text(String(localized: "Online")))
        .accessibilityValue(Text(String(format: String(localized: "%d up · %d total"), onlineCount, totalCount)))
    }

    var alertsTile: some View {
        StatTile(String(localized: "Alerts")) {
            if let firingAlertCount {
                StatValue(
                    value: firingAlertCount.formatted(),
                    color: firingAlertCount > 0 ? .alertFiring : .primary
                )
            } else {
                StatValue(value: "—", color: .secondary)
            }
        }
        .accessibilityLabel(Text(String(localized: "Alerts")))
        .accessibilityValue(Text(firingAlertCount.map { $0.formatted() } ?? String(localized: "Not available")))
    }

    var trafficTile: some View {
        let rate = ServerListRateFormat.split(downloadBytesPerSec)
        return StatTile("\(String(localized: "Traffic")) ↓") {
            StatValue(value: rate.value, suffix: rate.unit)
        }
        .accessibilityLabel(Text(String(localized: "Live traffic")))
        .accessibilityValue(Text(verbatim: "\(String(localized: "Download")) \(rate.value) \(rate.unit)"))
    }
}

#Preview {
    ServerListHeaderView(
        onlineCount: 5,
        totalCount: 6,
        firingAlertCount: 2,
        downloadBytesPerSec: 39_845_888
    )
    .padding()
    .background(Color(.systemGroupedBackground))
}
