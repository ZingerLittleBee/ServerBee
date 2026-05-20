import SwiftUI

/// A yellow warning banner shown when the configured server URL uses `http://`.
/// ATS is disabled globally in Info.plist because users self-host on arbitrary
/// IPs/domains, but we surface this trade-off to the user at runtime so they
/// can opt into HTTPS when possible. See `AppStoreReviewNotes.md` for the
/// App Store review justification.
struct InsecureURLBanner: View {
    let serverUrl: String

    var body: some View {
        if shouldShow {
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.yellow)
                Text(
                    String(
                        localized:
                            "This server uses an unencrypted HTTP connection. Credentials and metrics are sent in clear text. Use HTTPS whenever possible."
                    )
                )
                .font(.footnote)
                .foregroundStyle(.primary)
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.yellow.opacity(0.15), in: RoundedRectangle(cornerRadius: 8))
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(Color.yellow.opacity(0.5), lineWidth: 1)
            )
        }
    }

    private var shouldShow: Bool {
        let trimmed = serverUrl.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return trimmed.hasPrefix("http://")
    }
}

#Preview {
    VStack {
        InsecureURLBanner(serverUrl: "http://192.168.1.10:9527")
        InsecureURLBanner(serverUrl: "https://serverbee.example.com")
    }
    .padding()
}
