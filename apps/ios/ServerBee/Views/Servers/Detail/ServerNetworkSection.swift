import SwiftUI

// Placeholder — full implementation lands in the Network/Traceroute milestone.
struct ServerNetworkSection: View {
    let serverId: String
    let isAdmin: Bool

    var body: some View {
        ScrollView {
            ContentUnavailableView(
                String(localized: "Network"),
                systemImage: "dot.radiowaves.left.and.right",
                description: Text(String(localized: "Loading…"))
            )
            .padding(.top, 60)
        }
        .background(Color(.systemGroupedBackground))
    }
}
