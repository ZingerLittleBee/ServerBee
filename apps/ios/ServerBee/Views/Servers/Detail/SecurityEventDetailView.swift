import SwiftUI

/// Full detail for a single security event, presented as a sheet. Shows the
/// classification, source, detector, evidence breakdown, and a threat-intel
/// lookup link for the source IP. Admins can also jump straight to a firewall
/// block prefilled with the source IP (a high-risk action gated behind an
/// explicit confirmation sheet).
struct SecurityEventDetailView: View {
    let event: SecurityEvent
    /// Optional server label, shown when the event is viewed outside a single
    /// server's context (e.g. the fleet-wide security overview).
    var serverName: String?
    /// Called after a successful delete so the presenter can refresh its feed.
    var onDeleted: (() -> Void)?

    @Environment(\.dismiss) private var dismiss
    @Environment(AuthManager.self) private var authManager
    @Environment(\.apiClient) private var apiClient
    @State private var firewallViewModel = FirewallViewModel()
    @State private var actions = SecurityEventActionsViewModel()
    @State private var showBlockSheet = false
    @State private var showDeleteConfirm = false

    private var isAdmin: Bool { authManager.user?.role.lowercased() == "admin" }

    /// A source IP we can actually act on (non-empty, not a placeholder).
    private var blockableIp: String? {
        let ip = event.sourceIp.trimmingCharacters(in: .whitespaces)
        guard !ip.isEmpty, ip != "-", ip.lowercased() != "unknown" else { return nil }
        return ip
    }

    private var virusTotalURL: URL? {
        URL(string: "https://www.virustotal.com/gui/ip-address/\(event.sourceIp)")
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 16) {
                    headerCard
                    detailsCard
                    if let evidence = event.evidence, !evidence.detailRows.isEmpty {
                        evidenceCard(evidence)
                    }
                    if isAdmin, let ip = blockableIp {
                        blockActionCard(ip: ip)
                    }
                    if isAdmin {
                        deleteActionCard
                    }
                }
                .padding(16)
            }
            .background(Color(.systemGroupedBackground))
            .navigationTitle(String(localized: "Security Event"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(String(localized: "Done")) { dismiss() }
                }
            }
            .sheet(isPresented: $showBlockSheet) {
                if let ip = blockableIp {
                    AddBlockSheet(prefillTarget: ip) { request in
                        let ok = await firewallViewModel.create(request, apiClient: apiClient)
                        return ok ? nil : firewallViewModel.actionError
                    }
                }
            }
            .confirmationDialog(
                String(localized: "Delete this event?"),
                isPresented: $showDeleteConfirm,
                titleVisibility: .visible
            ) {
                Button(String(localized: "Delete"), role: .destructive) {
                    Task {
                        if await actions.delete(id: event.id, apiClient: apiClient) {
                            onDeleted?()
                            dismiss()
                        }
                    }
                }
                Button(String(localized: "Cancel"), role: .cancel) {}
            } message: {
                Text(String(localized: "This removes the event from history. It can't be undone."))
            }
        }
    }

    private var deleteActionCard: some View {
        SectionCard {
            VStack(alignment: .leading, spacing: 10) {
                if let error = actions.errorMessage {
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .font(.caption)
                        .foregroundStyle(Color.serverOffline)
                }
                Button(role: .destructive) {
                    showDeleteConfirm = true
                } label: {
                    HStack {
                        if actions.isWorking {
                            ProgressView().controlSize(.small)
                        }
                        Label(String(localized: "Delete event"), systemImage: "trash")
                            .font(.subheadline.weight(.semibold))
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .disabled(actions.isWorking)
            }
        }
    }

    private func blockActionCard(ip: String) -> some View {
        SectionCard {
            VStack(alignment: .leading, spacing: 10) {
                Button {
                    showBlockSheet = true
                } label: {
                    Label(String(localized: "Block \(ip) in firewall"), systemImage: "hand.raised.fill")
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(Color.serverOffline)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                Text(String(localized: "Adds a firewall blocklist rule. You'll choose the scope before it applies."))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var headerCard: some View {
        SectionCard {
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 10) {
                    Image(systemName: SecurityEventKind.icon(event.eventType))
                        .font(.title2)
                        .foregroundStyle(SecurityEventKind.color(event.eventType))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(SecurityEventKind.label(event.eventType))
                            .font(.headline)
                        if let serverName {
                            Label(serverName, systemImage: "server.rack")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        if let date = event.date {
                            Text(date, format: .dateTime.year().month().day().hour().minute().second())
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                    Spacer()
                    SeverityBadge(severity: event.severity)
                }
                if event.firstSeen {
                    Label(String(localized: "First time this source was seen"), systemImage: "sparkles")
                        .font(.caption)
                        .foregroundStyle(.blue)
                }
            }
        }
    }

    private var detailsCard: some View {
        SectionCard(String(localized: "Source"), systemImage: "network") {
            VStack(spacing: 8) {
                DetailRow(label: String(localized: "Source IP"), value: event.sourceIp, monospaced: true)
                if let port = event.sourcePort {
                    DetailRow(label: String(localized: "Port"), value: "\(port)", monospaced: true)
                }
                if let user = event.username {
                    DetailRow(label: String(localized: "Username"), value: user, monospaced: true)
                }
                DetailRow(label: String(localized: "Detector"), value: DetectorLabel.label(event.detectorSource))
                if let url = virusTotalURL {
                    Divider()
                    Link(destination: url) {
                        Label(String(localized: "Look up IP on VirusTotal"), systemImage: "arrow.up.forward.square")
                            .font(.subheadline)
                    }
                }
            }
        }
    }

    private func evidenceCard(_ evidence: SecurityEvidence) -> some View {
        SectionCard(String(localized: "Evidence"), systemImage: "doc.text.magnifyingglass") {
            VStack(spacing: 8) {
                ForEach(evidence.detailRows, id: \.0) { row in
                    DetailRow(label: row.0, value: row.1)
                }
            }
        }
    }
}
