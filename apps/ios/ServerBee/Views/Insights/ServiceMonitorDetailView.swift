import SwiftUI

/// Detail for a single service monitor: current status, configuration, recent
/// uptime, latest check detail, history, and an admin "check now" action.
struct ServiceMonitorDetailView: View {
    let monitor: ServiceMonitor
    let isAdmin: Bool

    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = ServiceMonitorDetailViewModel()

    private var current: ServiceMonitor { viewModel.detail?.monitor ?? monitor }

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                statusCard
                if isAdmin {
                    checkButton
                }
                if let record = viewModel.detail?.latestRecord {
                    latestCard(record)
                }
                configCard
                if !viewModel.records.isEmpty {
                    historyCard
                }
                if let error = viewModel.errorMessage {
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .font(.caption).foregroundStyle(Color.serverOffline)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .padding()
        }
        .background(Color(.systemGroupedBackground))
        .navigationTitle(current.name)
        .navigationBarTitleDisplayMode(.inline)
        .task { await viewModel.load(monitorId: monitor.id, apiClient: apiClient) }
    }

    private var statusCard: some View {
        SectionCard {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 10) {
                    MonitorStatusDot(status: current.isUp, disabled: !current.enabled)
                    Text(statusText).font(.headline).foregroundStyle(statusColor)
                    Spacer()
                    Chip(text: current.typeLabel, systemImage: current.typeIcon, color: .brandAccent)
                }
                Text(current.target).font(.callout.monospaced()).foregroundStyle(.secondary)
                    .textSelection(.enabled)
                if let uptime = viewModel.recentUptime {
                    Divider()
                    HStack {
                        Text(String(localized: "Recent uptime")).font(.caption).foregroundStyle(.secondary)
                        Spacer()
                        Text(Formatters.formatPercentage(uptime * 100))
                            .font(.subheadline.bold())
                            .foregroundStyle(Formatters.usageColor(for: (1 - uptime) * 100))
                    }
                }
            }
        }
    }

    private var statusText: String {
        if !current.enabled { return String(localized: "Paused") }
        switch current.isUp {
        case .some(true): return String(localized: "Operational")
        case .some(false): return String(localized: "Down")
        case .none: return String(localized: "Not checked yet")
        }
    }

    private var statusColor: Color {
        if !current.enabled { return .secondary }
        switch current.isUp {
        case .some(true): return .serverOnline
        case .some(false): return .serverOffline
        case .none: return .warningAmber
        }
    }

    private var checkButton: some View {
        Button {
            Task { await viewModel.runCheck(monitorId: monitor.id, apiClient: apiClient) }
        } label: {
            HStack {
                if viewModel.isChecking { ProgressView() } else { Image(systemName: "arrow.clockwise") }
                Text(String(localized: "Check now"))
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.borderedProminent)
        .disabled(viewModel.isChecking)
    }

    private func latestCard(_ record: ServiceMonitorRecord) -> some View {
        SectionCard(String(localized: "Latest check"), systemImage: "clock") {
            VStack(spacing: 8) {
                DetailRow(label: String(localized: "Result"),
                          value: record.success ? String(localized: "Success") : String(localized: "Failed"),
                          valueColor: record.success ? .serverOnline : .serverOffline)
                if let latency = record.latency {
                    DetailRow(label: String(localized: "Latency"), value: String(format: "%.0f ms", latency))
                }
                DetailRow(label: String(localized: "Time"), value: Formatters.formatRelativeTime(record.time))
                if let error = record.error {
                    DetailRow(label: String(localized: "Error"), value: error, valueColor: .serverOffline)
                }
            }
        }
    }

    private var configCard: some View {
        SectionCard(String(localized: "Configuration"), systemImage: "gearshape") {
            VStack(spacing: 8) {
                DetailRow(label: String(localized: "Interval"), value: "\(current.interval)s")
                DetailRow(label: String(localized: "Retries"), value: "\(current.retryCount)")
                DetailRow(label: String(localized: "Enabled"), value: current.enabled ? String(localized: "Yes") : String(localized: "No"))
                if let last = current.lastCheckedAt {
                    DetailRow(label: String(localized: "Last checked"), value: Formatters.formatRelativeTime(last))
                }
                if current.consecutiveFailures > 0 {
                    DetailRow(label: String(localized: "Consecutive failures"),
                              value: "\(current.consecutiveFailures)", valueColor: .serverOffline)
                }
            }
        }
    }

    private var historyCard: some View {
        let recent = Array(viewModel.records.prefix(30))
        return SectionCard(String(localized: "History"), systemImage: "list.bullet") {
            VStack(spacing: 0) {
                ForEach(recent) { record in
                    HStack(spacing: 10) {
                        Circle().fill(record.success ? Color.serverOnline : Color.serverOffline)
                            .frame(width: 8, height: 8)
                        Text(Formatters.formatRelativeTime(record.time)).font(.caption)
                        Spacer()
                        if let latency = record.latency {
                            Text(String(format: "%.0f ms", latency)).font(.caption.monospaced()).foregroundStyle(.secondary)
                        } else if let error = record.error {
                            Text(error).font(.caption2).foregroundStyle(Color.serverOffline).lineLimit(1)
                        }
                    }
                    .padding(.vertical, 6)
                    if record.id != recent.last?.id { Divider() }
                }
            }
        }
    }
}
