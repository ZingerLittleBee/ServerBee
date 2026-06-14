import SwiftUI

// Placeholder — full implementation lands in the Traffic/Cost/Uptime milestone.
struct ServerTrafficSection: View {
    let serverId: String
    let config: ServerConfig?

    var body: some View {
        ScrollView {
            ContentUnavailableView(
                String(localized: "Traffic"),
                systemImage: "chart.bar",
                description: Text(String(localized: "Loading…"))
            )
            .padding(.top, 60)
        }
        .background(Color(.systemGroupedBackground))
    }
}
