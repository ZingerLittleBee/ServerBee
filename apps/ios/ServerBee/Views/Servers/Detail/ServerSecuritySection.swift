import SwiftUI

// Placeholder — full implementation lands in the Security Events milestone.
struct ServerSecuritySection: View {
    let serverId: String

    var body: some View {
        ScrollView {
            ContentUnavailableView(
                String(localized: "Security"),
                systemImage: "shield",
                description: Text(String(localized: "Loading…"))
            )
            .padding(.top, 60)
        }
        .background(Color(.systemGroupedBackground))
    }
}
