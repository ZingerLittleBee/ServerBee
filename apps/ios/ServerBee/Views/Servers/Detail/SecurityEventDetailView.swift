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
    @Environment(\.privacyMode) private var privacyMode
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
                VStack(alignment: .leading, spacing: 16) {
                    header
                    sourceGroup
                    if let evidence = event.evidence, !evidence.detailRows.isEmpty {
                        evidenceGroup(evidence)
                    }
                    if isAdmin {
                        adminActions
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
}

// MARK: - Sections

private extension SecurityEventDetailView {
    /// "tokyo-edge-01 · Sep 28, 2026 at 14:02:11" — whichever parts are known.
    var subtitle: String? {
        let date = event.date?.formatted(.dateTime.year().month().day().hour().minute().second())
        let parts = [serverName, date].compactMap { $0 }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    var header: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 12) {
                SecurityKindIcon(eventType: event.eventType, baseSize: 44)
                VStack(alignment: .leading, spacing: 4) {
                    Text(SecurityEventKind.label(event.eventType))
                        .font(.title2.bold())
                        .accessibilityAddTraits(.isHeader)
                    SeverityBadge(severity: event.severity)
                }
            }
            if let subtitle {
                Text(verbatim: subtitle)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            if event.firstSeen {
                Label(String(localized: "First time this source was seen"), systemImage: "sparkles")
                    .font(.footnote.weight(.medium))
                    .foregroundStyle(Color.accentColor)
            }
        }
        .padding(.horizontal, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    var sourceItems: [EventInfoItem] {
        var items = [EventInfoItem(label: String(localized: "Source IP"), value: event.sourceIp.maskingIPs(privacyMode), monospaced: true)]
        if let port = event.sourcePort {
            items.append(EventInfoItem(label: String(localized: "Port"), value: "\(port)", monospaced: true))
        }
        if let user = event.username {
            items.append(EventInfoItem(label: String(localized: "Username"), value: user, monospaced: true))
        }
        items.append(EventInfoItem(label: String(localized: "Detector"), value: DetectorLabel.label(event.detectorSource)))
        return items
    }

    var sourceGroup: some View {
        VStack(alignment: .leading, spacing: 8) {
            GroupHeader(String(localized: "Source"))
            VStack(spacing: 0) {
                EventInfoRows(items: sourceItems)
                if let url = virusTotalURL {
                    Divider().padding(.leading, 16)
                    Link(destination: url) {
                        Label(String(localized: "Look up IP on VirusTotal"), systemImage: "arrow.up.forward.square")
                            .font(.subheadline)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 16)
                            .frame(minHeight: 44)
                            .contentShape(Rectangle())
                    }
                }
            }
            .cardSurface(padding: 0)
        }
    }

    func evidenceGroup(_ evidence: SecurityEvidence) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            GroupHeader(String(localized: "Evidence"))
            EventInfoRows(items: evidence.detailRows.map { EventInfoItem(label: $0.0, value: $0.1) })
                .cardSurface(padding: 0)
        }
    }
}

// MARK: - Admin actions

private extension SecurityEventDetailView {
    var adminActions: some View {
        VStack(alignment: .leading, spacing: 8) {
            GroupHeader(String(localized: "Actions"))
            VStack(alignment: .leading, spacing: 16) {
                if let ip = blockableIp {
                    blockAction(ip: ip)
                }
                deleteAction
            }
        }
    }

    func blockAction(ip: String) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Button {
                showBlockSheet = true
            } label: {
                Label(String(localized: "Block \(ip.maskingIPs(privacyMode)) in firewall"), systemImage: "hand.raised.fill")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Color.serverOffline)
                    .actionRowLayout()
            }
            .buttonStyle(.borderless)
            .cardSurface(padding: 0)
            footer(String(localized: "Adds a firewall blocklist rule. You'll choose the scope before it applies."))
        }
    }

    var deleteAction: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button(role: .destructive) {
                showDeleteConfirm = true
            } label: {
                HStack(spacing: 8) {
                    Label(String(localized: "Delete event"), systemImage: "trash")
                        .font(.subheadline.weight(.semibold))
                    if actions.isWorking {
                        ProgressView().controlSize(.small)
                    }
                }
                .actionRowLayout()
            }
            .buttonStyle(.borderless)
            .disabled(actions.isWorking)
            .cardSurface(padding: 0)
            if let error = actions.errorMessage {
                Label(error, systemImage: "exclamationmark.triangle.fill")
                    .font(.footnote)
                    .foregroundStyle(Color.serverOffline)
                    .padding(.horizontal, 16)
            }
        }
    }

    func footer(_ text: String) -> some View {
        Text(text)
            .font(.footnote)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 16)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private extension View {
    /// Full-width, leading-aligned tappable row inside a grouped card.
    func actionRowLayout() -> some View {
        self
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 16)
            .frame(minHeight: 44)
            .contentShape(Rectangle())
    }
}

// MARK: - Info rows

/// A label → value pair shown as one row of a grouped card.
private struct EventInfoItem: Identifiable {
    let label: String
    let value: String
    var monospaced = false

    var id: String { label }
}

/// Rows of a grouped card separated by hairlines inset from the leading edge.
private struct EventInfoRows: View {
    let items: [EventInfoItem]

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                if index > 0 {
                    Divider().padding(.leading, 16)
                }
                EventInfoRow(item: item)
            }
        }
    }
}

/// Inset-grouped style row: primary label on the leading edge, secondary
/// selectable value trailing. At accessibility text sizes the value stacks
/// under the label so IPs and lists don't break mid-token.
private struct EventInfoRow: View {
    let item: EventInfoItem

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        Group {
            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 4) {
                    label
                    value.multilineTextAlignment(.leading)
                }
            } else {
                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    label
                    Spacer(minLength: 12)
                    value.multilineTextAlignment(.trailing)
                }
            }
        }
        .font(.subheadline)
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(item.label))
        .accessibilityValue(Text(item.value))
    }

    private var label: some View {
        Text(item.label)
            .foregroundStyle(.primary)
    }

    private var value: some View {
        Text(item.value)
            .font(item.monospaced ? .subheadline.monospaced() : .subheadline)
            .foregroundStyle(.secondary)
            .textSelection(.enabled)
    }
}
