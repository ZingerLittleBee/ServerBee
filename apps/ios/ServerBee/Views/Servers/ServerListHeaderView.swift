import SwiftUI

/// A reusable header component for the servers list, containing:
/// - Server count summary
/// - Online/Offline/All filter segmented control
struct ServerListHeaderView: View {
    @Binding var filter: OnlineFilter
    let totalCount: Int
    let onlineCount: Int

    var body: some View {
        VStack(spacing: 12) {
            // Server count summary
            HStack {
                Text(String(localized: "\(totalCount) servers"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Spacer()
                HStack(spacing: 4) {
                    Circle()
                        .fill(Color.serverOnline)
                        .frame(width: 8, height: 8)
                    Text(String(localized: "\(onlineCount) online"))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
            }

            // Filter segmented control
            Picker(String(localized: "Filter"), selection: $filter) {
                ForEach(OnlineFilter.allCases, id: \.self) { filter in
                    Text(filter.displayName).tag(filter)
                }
            }
            .pickerStyle(.segmented)
        }
    }
}

#Preview {
    ServerListHeaderView(
        filter: .constant(.all),
        totalCount: 5,
        onlineCount: 3
    )
    .padding()
}
