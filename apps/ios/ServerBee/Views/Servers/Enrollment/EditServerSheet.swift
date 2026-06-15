import SwiftUI

/// Admin "Edit server" form. Prefills from the server's REST config and PUTs
/// `/api/servers/{id}` (+ `/tags`). Capability editing is intentionally out of
/// scope (managed elsewhere).
struct EditServerSheet: View {
    let serverId: String
    let config: ServerConfig
    let onSaved: () -> Void

    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss
    @State private var viewModel = EditServerViewModel()

    private static let currencies = ["USD", "EUR", "CNY", "JPY", "GBP"]
    private static let cycles = ["monthly", "quarterly", "yearly"]
    private static let trafficTypes = ["sum", "up", "down"]

    var body: some View {
        NavigationStack {
            Form {
                basicSection
                billingSection
                if let error = viewModel.errorMessage {
                    Section {
                        Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline)
                    }
                }
            }
            .navigationTitle(String(localized: "Edit Server"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbarContent }
            .task {
                viewModel.prefill(from: config)
                await viewModel.loadGroups(apiClient: apiClient)
                await viewModel.loadTags(serverId: serverId, apiClient: apiClient)
            }
        }
    }
}

private extension EditServerSheet {
    @ViewBuilder
    var basicSection: some View {
        Section {
            TextField(String(localized: "Name"), text: $viewModel.name)
            Picker(String(localized: "Group"), selection: $viewModel.groupId) {
                Text(String(localized: "No group")).tag("")
                ForEach(viewModel.groups) { group in
                    Text(group.name).tag(group.id)
                }
            }
            TextField(String(localized: "Tags (comma separated)"), text: $viewModel.tagsText)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
            TextField(String(localized: "Remark"), text: $viewModel.remark, axis: .vertical).lineLimit(1...3)
            TextField(String(localized: "Public remark"), text: $viewModel.publicRemark, axis: .vertical).lineLimit(1...3)
            Toggle(String(localized: "Hidden"), isOn: $viewModel.hidden)
            Stepper(String(format: String(localized: "Weight: %d"), viewModel.weight), value: $viewModel.weight, in: 0...9999)
        } header: {
            Text(String(localized: "Basic"))
        } footer: {
            Text(String(localized: "Up to 8 tags, each ≤16 chars (letters, digits, _ - .)."))
        }
    }

    @ViewBuilder
    var billingSection: some View {
        Section(String(localized: "Billing")) {
            TextField(String(localized: "Price"), text: $viewModel.priceText).keyboardType(.decimalPad)
            Picker(String(localized: "Currency"), selection: $viewModel.currency) {
                Text(String(localized: "None")).tag("")
                ForEach(Self.currencies, id: \.self) { Text($0).tag($0) }
            }
            Picker(String(localized: "Cycle"), selection: $viewModel.billingCycle) {
                Text(String(localized: "None")).tag("")
                ForEach(Self.cycles, id: \.self) { Text($0.capitalized).tag($0) }
            }
            TextField(String(localized: "Billing day (1-28)"), text: $viewModel.billingStartDayText)
                .keyboardType(.numberPad)
            Toggle(String(localized: "Has expiry"), isOn: $viewModel.hasExpiry)
            if viewModel.hasExpiry {
                DatePicker(String(localized: "Expires"), selection: $viewModel.expiryDate, displayedComponents: .date)
            }
            TextField(String(localized: "Traffic limit (GiB)"), text: $viewModel.trafficLimitGiBText)
                .keyboardType(.decimalPad)
            Picker(String(localized: "Traffic type"), selection: $viewModel.trafficLimitType) {
                Text(String(localized: "None")).tag("")
                ForEach(Self.trafficTypes, id: \.self) { Text($0.capitalized).tag($0) }
            }
        }
    }

    @ToolbarContentBuilder
    var toolbarContent: some ToolbarContent {
        ToolbarItem(placement: .cancellationAction) {
            Button(String(localized: "Cancel")) { dismiss() }
        }
        ToolbarItem(placement: .confirmationAction) {
            if viewModel.isSaving {
                ProgressView()
            } else {
                Button(String(localized: "Save")) {
                    Task {
                        if await viewModel.save(serverId: serverId, apiClient: apiClient) {
                            onSaved()
                            dismiss()
                        }
                    }
                }
                .disabled(viewModel.name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }
}
