import SwiftUI

/// Sheet to create a firewall block. Used both from the blocklist screen
/// (empty target) and from a security event ("Block this IP", prefilled).
///
/// High-risk action: the form makes the *scope* explicit (all servers vs a
/// chosen subset) before the user can submit, and surfaces server-side
/// validation errors inline rather than failing silently.
struct AddBlockSheet: View {
    /// Optional prefilled target (e.g. a security event's source IP).
    var prefillTarget: String = ""
    /// Returns nil on success (sheet dismisses); otherwise an error message to
    /// surface inline.
    let onSubmit: (CreateBlockRequest) async -> String?

    @Environment(\.dismiss) private var dismiss
    @Environment(ServersViewModel.self) private var serversViewModel

    @State private var target = ""
    @State private var scope: Scope = .all
    @State private var selectedServerIds: Set<String> = []
    @State private var comment = ""
    @State private var submitting = false
    @State private var errorMessage: String?

    /// Mobile-appropriate subset of the server's cover types. "Exclude" is a
    /// rarely-used advanced mode left to the web console.
    private enum Scope: String, CaseIterable, Identifiable {
        case all
        case specific
        var id: String { rawValue }
        var label: String {
            switch self {
            case .all: String(localized: "All servers")
            case .specific: String(localized: "Specific servers")
            }
        }
    }

    private var trimmedTarget: String {
        target.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var canSubmit: Bool {
        guard !trimmedTarget.isEmpty, !submitting else { return false }
        if scope == .specific { return !selectedServerIds.isEmpty }
        return true
    }

    var body: some View {
        NavigationStack {
            Form {
                targetSection
                scopeSection
                if scope == .specific {
                    serverSelectionSection
                }
                commentSection
                if let errorMessage {
                    Section {
                        Label(errorMessage, systemImage: "exclamationmark.triangle.fill")
                            .font(.subheadline)
                            .foregroundStyle(Color.serverOffline)
                    }
                }
            }
            .navigationTitle(String(localized: "Block Target"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(String(localized: "Cancel")) { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if submitting {
                        ProgressView()
                    } else {
                        Button(String(localized: "Block")) { Task { await submit() } }
                            .disabled(!canSubmit)
                    }
                }
            }
            .onAppear {
                if target.isEmpty { target = prefillTarget }
            }
        }
    }

    private var targetSection: some View {
        Section {
            TextField(String(localized: "IP or CIDR (e.g. 1.2.3.4 or 1.2.3.0/24)"), text: $target)
                .font(.body.monospaced())
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.numbersAndPunctuation)
        } header: {
            Text(String(localized: "Target"))
        } footer: {
            Text(String(localized: "A single IPv4/IPv6 address or a CIDR range. Loopback, private and metadata addresses are rejected by the server."))
        }
    }

    private var scopeSection: some View {
        Section(String(localized: "Scope")) {
            Picker(String(localized: "Apply to"), selection: $scope) {
                ForEach(Scope.allCases) { s in
                    Text(s.label).tag(s)
                }
            }
            .pickerStyle(.segmented)
        }
    }

    private var serverSelectionSection: some View {
        Section {
            if serversViewModel.servers.isEmpty {
                Text(String(localized: "No servers available"))
                    .foregroundStyle(.secondary)
            } else {
                ForEach(serversViewModel.servers) { server in
                    Button {
                        toggle(server.id)
                    } label: {
                        HStack {
                            Text(server.name)
                                .foregroundStyle(.primary)
                            Spacer()
                            if selectedServerIds.contains(server.id) {
                                Image(systemName: "checkmark")
                                    .foregroundStyle(Color.accentColor)
                            }
                        }
                    }
                }
            }
        } header: {
            Text(String(localized: "Servers"))
        } footer: {
            Text(String(localized: "The block applies only to the selected servers."))
        }
    }

    private var commentSection: some View {
        Section(String(localized: "Note (optional)")) {
            TextField(String(localized: "Why is this blocked?"), text: $comment, axis: .vertical)
                .lineLimit(1...3)
        }
    }

    private func toggle(_ id: String) {
        if selectedServerIds.contains(id) {
            selectedServerIds.remove(id)
        } else {
            selectedServerIds.insert(id)
        }
    }

    private func submit() async {
        submitting = true
        errorMessage = nil
        let trimmedComment = comment.trimmingCharacters(in: .whitespacesAndNewlines)
        let request = CreateBlockRequest(
            target: trimmedTarget,
            coverType: scope == .all ? BlockCoverType.all.rawValue : BlockCoverType.include.rawValue,
            serverIds: scope == .specific ? Array(selectedServerIds) : nil,
            comment: trimmedComment.isEmpty ? nil : trimmedComment
        )
        let failure = await onSubmit(request)
        submitting = false
        if let failure {
            errorMessage = failure
        } else {
            dismiss()
        }
    }
}
