import SwiftUI

/// Create or edit a custom network-probe target (admin). Preset targets are not
/// editable, so this sheet is only opened for custom ones (or a fresh create).
struct NetworkProbeTargetFormSheet: View {
    let editing: NetworkProbeTarget?
    @Bindable var viewModel: NetworkProbeConfigViewModel
    let onSaved: () -> Void

    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss

    @State private var name = ""
    @State private var provider = ""
    @State private var location = ""
    @State private var target = ""
    @State private var probeType: PingProbeType = .icmp
    @State private var saving = false

    private var isValid: Bool {
        ![name, provider, location, target].contains {
            $0.trimmingCharacters(in: .whitespaces).isEmpty
        }
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(String(localized: "Name"), text: $name)
                    TextField(String(localized: "Provider"), text: $provider)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                    TextField(String(localized: "Location"), text: $location)
                } footer: {
                    Text(String(localized: "Provider groups targets on the network screen (e.g. ct, cu, cm, international)."))
                }
                Section {
                    Picker(String(localized: "Type"), selection: $probeType) {
                        ForEach(PingProbeType.allCases) { Text($0.label).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    TextField(probeType.targetPlaceholder, text: $target)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .keyboardType(.URL)
                }
                if let error = viewModel.actionError {
                    Section {
                        Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline)
                    }
                }
            }
            .navigationTitle(editing == nil ? String(localized: "New Target") : String(localized: "Edit Target"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbarContent }
            .onAppear(perform: prefill)
        }
    }

    @ToolbarContentBuilder
    private var toolbarContent: some ToolbarContent {
        ToolbarItem(placement: .cancellationAction) {
            Button(String(localized: "Cancel")) { dismiss() }
        }
        ToolbarItem(placement: .confirmationAction) {
            if saving {
                ProgressView()
            } else {
                Button(String(localized: "Save")) { Task { await save() } }
                    .disabled(!isValid)
            }
        }
    }

    private func prefill() {
        guard let editing else { return }
        name = editing.name
        provider = editing.provider
        location = editing.location
        target = editing.target
        probeType = editing.probeTypeEnum
    }

    private func save() async {
        saving = true
        defer { saving = false }
        let n = name.trimmingCharacters(in: .whitespaces)
        let p = provider.trimmingCharacters(in: .whitespaces)
        let l = location.trimmingCharacters(in: .whitespaces)
        let t = target.trimmingCharacters(in: .whitespaces)
        let failure: String?
        if let editing {
            failure = await viewModel.updateTarget(
                id: editing.id,
                UpdateProbeTargetRequest(name: n, provider: p, location: l, target: t, probeType: probeType.rawValue),
                apiClient: apiClient
            )
        } else {
            failure = await viewModel.createTarget(
                CreateProbeTargetRequest(name: n, provider: p, location: l, target: t, probeType: probeType.rawValue),
                apiClient: apiClient
            )
        }
        if failure == nil {
            onSaved()
            dismiss()
        }
    }
}
