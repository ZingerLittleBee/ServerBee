import SwiftUI

/// Full list of service monitors with status, grouped visually by up/down.
struct ServiceMonitorsView: View {
    let monitors: [ServiceMonitor]
    let isAdmin: Bool

    var body: some View {
        Group {
            if monitors.isEmpty {
                ContentUnavailableView {
                    Label(String(localized: "No monitors"), systemImage: "checkmark.shield")
                } description: {
                    Text(String(localized: "Service monitors are configured from the web dashboard."))
                }
            } else {
                ScrollView {
                    VStack(spacing: 10) {
                        ForEach(monitors) { monitor in
                            NavigationLink {
                                ServiceMonitorDetailView(monitor: monitor, isAdmin: isAdmin)
                            } label: {
                                ServiceMonitorRow(monitor: monitor)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding()
                }
                .background(Color(.systemGroupedBackground))
            }
        }
        .navigationTitle(String(localized: "Monitors"))
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct ServiceMonitorRow: View {
    let monitor: ServiceMonitor

    var body: some View {
        SectionCard {
            HStack(spacing: 12) {
                MonitorStatusDot(status: monitor.isUp, disabled: !monitor.enabled)
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 6) {
                        Text(monitor.name).font(.subheadline.bold()).lineLimit(1)
                        Chip(text: monitor.typeLabel, systemImage: monitor.typeIcon, color: .brandAccent)
                    }
                    Text(monitor.target).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer(minLength: 4)
                if !monitor.enabled {
                    Text(String(localized: "Paused")).font(.caption2).foregroundStyle(.secondary)
                }
                Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
            }
        }
    }
}

/// Up/down/unknown status indicator for a monitor.
struct MonitorStatusDot: View {
    let status: Bool?
    var disabled = false

    private var color: Color {
        if disabled { return .secondary }
        switch status {
        case .some(true): return .serverOnline
        case .some(false): return .serverOffline
        case .none: return .warningAmber
        }
    }

    var body: some View {
        Circle().fill(color).frame(width: 12, height: 12)
            .accessibilityLabel(Text(disabled ? String(localized: "Paused")
                : status == true ? String(localized: "Up")
                : status == false ? String(localized: "Down")
                : String(localized: "Unknown")))
    }
}
