import SwiftUI

/// A banner displayed at the top of the screen when the device is offline.
struct OfflineBannerView: View {
    let isConnected: Bool

    var body: some View {
        if !isConnected {
            HStack(spacing: 6) {
                Image(systemName: "wifi.slash")
                Text("You are currently offline")
            }
            .font(.subheadline)
            .foregroundStyle(.black)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 8)
            .background(Color.yellow.opacity(0.9))
        }
    }
}

#Preview("Offline") {
    OfflineBannerView(isConnected: false)
}

#Preview("Online") {
    OfflineBannerView(isConnected: true)
}
