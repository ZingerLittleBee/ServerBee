import SwiftUI

/// Full detail for a single security event, presented as a sheet. Shows the
/// classification, source, detector, evidence breakdown, and a threat-intel
/// lookup link for the source IP. Blocking the IP is a firewall action handled
/// in the Management area, so this view stays read-only.
struct SecurityEventDetailView: View {
    let event: SecurityEvent

    @Environment(\.dismiss) private var dismiss

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
