import SwiftUI

/// Admin-only management hub pushed from Settings → Administration. Groups the
/// fleet, reputation, public, people and data configuration screens; every row
/// pushes the same destination Settings previously listed inline.
struct AdministrationView: View {
    let isAdmin: Bool

    var body: some View {
        List {
            fleetSection
            reputationSection
            publicSection
            peopleSection
            dataSection
        }
        .listStyle(.insetGrouped)
        .navigationTitle(String(localized: "Administration"))
        .navigationBarTitleDisplayMode(.large)
    }
}

private extension AdministrationView {
    var fleetSection: some View {
        Section(String(localized: "Fleet")) {
            NavigationLink {
                ServerGroupsView()
            } label: {
                IconRowLabel(title: String(localized: "Server Groups"), systemImage: "folder.fill", color: .blue)
            }
            NavigationLink {
                PingTasksView(isAdmin: isAdmin)
            } label: {
                IconRowLabel(
                    title: String(localized: "Ping Tasks"),
                    systemImage: "dot.radiowaves.left.and.right",
                    color: .green
                )
            }
            NavigationLink {
                TasksView(isAdmin: isAdmin)
            } label: {
                IconRowLabel(title: String(localized: "Scheduled Commands"), systemImage: "terminal.fill", color: .gray)
            }
            NavigationLink {
                NetworkProbeConfigView(isAdmin: isAdmin)
            } label: {
                IconRowLabel(
                    title: String(localized: "Network Probes"),
                    systemImage: "point.3.connected.trianglepath.dotted",
                    color: .orange
                )
            }
        }
    }

    var reputationSection: some View {
        Section(String(localized: "Reputation & Limits")) {
            NavigationLink {
                IpQualityConfigView(isAdmin: isAdmin)
            } label: {
                IconRowLabel(title: String(localized: "IP Quality"), systemImage: "checkmark.shield.fill", color: .teal)
            }
            NavigationLink {
                RateLimitView()
            } label: {
                IconRowLabel(title: String(localized: "Rate Limits"), systemImage: "gauge.with.needle", color: .red)
            }
        }
    }

    var publicSection: some View {
        Section(String(localized: "Public")) {
            NavigationLink {
                StatusPageConfigView(isAdmin: isAdmin)
            } label: {
                IconRowLabel(title: String(localized: "Status Page"), systemImage: "globe.americas", color: .indigo)
            }
        }
    }

    var peopleSection: some View {
        Section(String(localized: "People")) {
            NavigationLink {
                UsersView()
            } label: {
                IconRowLabel(title: String(localized: "Users"), systemImage: "person.2.fill", color: .blue)
            }
            NavigationLink {
                AuditLogView()
            } label: {
                IconRowLabel(title: String(localized: "Audit Log"), systemImage: "list.bullet", color: .gray)
            }
        }
    }

    var dataSection: some View {
        Section(String(localized: "Data")) {
            NavigationLink {
                DatabasesView(isAdmin: isAdmin)
            } label: {
                IconRowLabel(title: String(localized: "GeoIP & ASN"), systemImage: "globe", color: .purple)
            }
        }
    }
}
