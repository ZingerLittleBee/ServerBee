import SwiftUI

/// Operational status: active + recent incidents and scheduled maintenance.
/// Admins can open incidents and post status updates (including resolving).
struct IncidentsView: View {
    @Bindable var viewModel: InsightsViewModel
    let isAdmin: Bool

    @Environment(\.apiClient) private var apiClient
    @State private var actions = IncidentActionsViewModel()
    @State private var showCreate = false
    @State private var updateTarget: Incident?

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                if viewModel.activeIncidents.isEmpty && viewModel.recentResolved.isEmpty && viewModel.maintenances.isEmpty {
                    allClear
                }
                if !viewModel.activeIncidents.isEmpty {
                    section(String(localized: "Active incidents")) {
                        ForEach(viewModel.activeIncidents) { incident in
                            incidentCard(incident)
                        }
                    }
                }
                if !viewModel.upcomingMaintenances.isEmpty {
                    section(String(localized: "Maintenance")) {
                        ForEach(viewModel.upcomingMaintenances) { maintenance in
                            maintenanceCard(maintenance)
                        }
                    }
                }
                if !viewModel.recentResolved.isEmpty {
                    section(String(localized: "Recently resolved")) {
                        ForEach(viewModel.recentResolved) { incident in
                            incidentCard(incident)
                        }
                    }
                }
            }
            .padding()
        }
        .background(Color(.systemGroupedBackground))
        .navigationTitle(String(localized: "Status"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if isAdmin {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { showCreate = true } label: {
                        Label(String(localized: "New incident"), systemImage: "plus")
                    }
                }
            }
        }
        .sheet(isPresented: $showCreate, onDismiss: reload) {
            CreateIncidentSheet(actions: actions)
        }
        .sheet(item: $updateTarget, onDismiss: reload) { incident in
            IncidentUpdateSheet(incident: incident, actions: actions)
        }
        #if DEBUG
        .task {
            if isAdmin, UITestSupport.autoPresent == "insights-incidents-create" { showCreate = true }
        }
        #endif
    }

    private var allClear: some View {
        ContentUnavailableView {
            Label(String(localized: "All systems operational"), systemImage: "checkmark.circle.fill")
        } description: {
            Text(String(localized: "No active incidents or scheduled maintenance."))
        }
        .frame(minHeight: 240)
    }

    @ViewBuilder
    private func section(_ title: String, @ViewBuilder content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title).font(.headline).frame(maxWidth: .infinity, alignment: .leading)
            content()
        }
    }

    private func incidentCard(_ incident: Incident) -> some View {
        SectionCard {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text(incident.title).font(.subheadline.bold())
                    Spacer()
                    Chip(text: incident.severity.capitalized, color: incident.severityColor)
                }
                HStack(spacing: 8) {
                    Chip(text: incident.statusLabel, color: incident.isResolved ? .serverOnline : .warningAmber)
                    if incident.isPublic {
                        Chip(text: String(localized: "Public"), systemImage: "globe", color: .brandAccent)
                    }
                    Spacer()
                    Text(Formatters.formatRelativeTime(incident.createdAt))
                        .font(.caption2).foregroundStyle(.secondary)
                }
                if isAdmin, !incident.isResolved {
                    Divider()
                    Button {
                        updateTarget = incident
                    } label: {
                        Label(String(localized: "Post update / resolve"), systemImage: "text.bubble")
                            .font(.caption)
                    }
                }
            }
        }
    }

    private func maintenanceCard(_ maintenance: Maintenance) -> some View {
        SectionCard {
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Text(maintenance.title).font(.subheadline.bold())
                    Spacer()
                    if maintenance.isPublic {
                        Chip(text: String(localized: "Public"), systemImage: "globe", color: .brandAccent)
                    }
                }
                if let desc = maintenance.description, !desc.isEmpty {
                    Text(desc).font(.caption).foregroundStyle(.secondary)
                }
                Text(String(format: String(localized: "%@ → %@"),
                            Formatters.formatRelativeTime(maintenance.startAt),
                            Formatters.formatRelativeTime(maintenance.endAt)))
                    .font(.caption2).foregroundStyle(.secondary)
            }
        }
    }

    private func reload() {
        Task { await viewModel.load(apiClient: apiClient) }
    }
}

// MARK: - Create incident

struct CreateIncidentSheet: View {
    @Bindable var actions: IncidentActionsViewModel
    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss

    @State private var title = ""
    @State private var severity: IncidentSeverity = .minor
    @State private var isPublic = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(String(localized: "Title"), text: $title)
                    Picker(String(localized: "Severity"), selection: $severity) {
                        ForEach(IncidentSeverity.allCases) { Text($0.label).tag($0) }
                    }
                    Toggle(String(localized: "Show on public status page"), isOn: $isPublic)
                }
                if let error = actions.errorMessage {
                    Section { Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline) }
                }
            }
            .navigationTitle(String(localized: "New Incident"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(String(localized: "Cancel")) { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if actions.isWorking { ProgressView() } else {
                        Button(String(localized: "Create")) {
                            Task {
                                if await actions.create(title: title.trimmingCharacters(in: .whitespaces),
                                                        severity: severity, isPublic: isPublic, apiClient: apiClient) {
                                    dismiss()
                                }
                            }
                        }
                        .disabled(title.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                }
            }
        }
    }
}

// MARK: - Post update / resolve

struct IncidentUpdateSheet: View {
    let incident: Incident
    @Bindable var actions: IncidentActionsViewModel
    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss

    @State private var status: IncidentStatus = .investigating
    @State private var message = ""

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker(String(localized: "Status"), selection: $status) {
                        ForEach(IncidentStatus.allCases) { Text($0.label).tag($0) }
                    }
                    TextField(String(localized: "Update message"), text: $message, axis: .vertical)
                        .lineLimit(3...6)
                } header: {
                    Text(incident.title)
                } footer: {
                    Text(String(localized: "Setting the status to Resolved closes the incident."))
                }
                if let error = actions.errorMessage {
                    Section { Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline) }
                }
            }
            .navigationTitle(String(localized: "Post Update"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(String(localized: "Cancel")) { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if actions.isWorking { ProgressView() } else {
                        Button(String(localized: "Post")) {
                            Task {
                                if await actions.addUpdate(incidentId: incident.id, status: status,
                                                           message: message.trimmingCharacters(in: .whitespaces),
                                                           apiClient: apiClient) {
                                    dismiss()
                                }
                            }
                        }
                        .disabled(message.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                }
            }
        }
    }
}
