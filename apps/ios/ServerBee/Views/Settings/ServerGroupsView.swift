import SwiftUI

/// Admin management of server groups: list, create, rename, delete.
struct ServerGroupsView: View {
    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = ServerGroupsViewModel()
    @State private var showCreate = false
    @State private var renameTarget: ServerGroup?
    @State private var pendingDelete: ServerGroup?

    var body: some View {
        List {
            if let error = viewModel.actionError {
                Section {
                    Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline)
                }
            }
            if viewModel.groups.isEmpty, !viewModel.isLoading {
                Section {
                    Text(String(localized: "No groups yet.")).foregroundStyle(.secondary)
                }
            }
            ForEach(viewModel.groups) { group in
                HStack {
                    Image(systemName: "folder").foregroundStyle(Color.brandAccent)
                    Text(group.name)
                }
                .swipeActions {
                    Button(role: .destructive) { pendingDelete = group } label: {
                        Label(String(localized: "Delete"), systemImage: "trash")
                    }
                    Button { renameTarget = group } label: {
                        Label(String(localized: "Rename"), systemImage: "pencil")
                    }
                    .tint(.brandAccent)
                }
            }
        }
        .overlay {
            if viewModel.isLoading, viewModel.groups.isEmpty { ProgressView() }
        }
        .navigationTitle(String(localized: "Server Groups"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { showCreate = true } label: { Image(systemName: "plus") }
                    .accessibilityLabel(String(localized: "Add"))
            }
        }
        .task { await viewModel.load(apiClient: apiClient) }
        .refreshable { await viewModel.load(apiClient: apiClient) }
        .sheet(isPresented: $showCreate) {
            GroupNameSheet(title: String(localized: "New Group"), initial: "") { name in
                await viewModel.create(name: name, apiClient: apiClient)
            }
        }
        .sheet(item: $renameTarget) { group in
            GroupNameSheet(title: String(localized: "Rename Group"), initial: group.name) { name in
                await viewModel.rename(id: group.id, name: name, apiClient: apiClient)
            }
        }
        .confirmationDialog(
            String(localized: "Delete this group?"),
            isPresented: Binding(get: { pendingDelete != nil }, set: { if !$0 { pendingDelete = nil } }),
            titleVisibility: .visible
        ) {
            if let group = pendingDelete {
                Button(String(format: String(localized: "Delete %@"), group.name), role: .destructive) {
                    Task { await viewModel.delete(id: group.id, apiClient: apiClient) }
                }
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: {
            Text(String(localized: "Servers in this group become ungrouped. This cannot be undone."))
        }
    }
}

/// Reusable name-entry sheet for create/rename. The action returns a localized
/// error string on failure (nil = success → dismiss).
private struct GroupNameSheet: View {
    let title: String
    let initial: String
    let action: (String) async -> String?

    @Environment(\.dismiss) private var dismiss
    @State private var name: String
    @State private var error: String?
    @State private var working = false

    init(title: String, initial: String, action: @escaping (String) async -> String?) {
        self.title = title
        self.initial = initial
        self.action = action
        _name = State(initialValue: initial)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(String(localized: "Group name"), text: $name)
                        .autocorrectionDisabled()
                }
                if let error {
                    Section {
                        Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline)
                    }
                }
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(String(localized: "Cancel")) { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if working {
                        ProgressView()
                    } else {
                        Button(String(localized: "Save")) { Task { await submit() } }
                            .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                }
            }
        }
    }

    private func submit() async {
        working = true
        error = nil
        let failure = await action(name.trimmingCharacters(in: .whitespaces))
        working = false
        if let failure {
            error = failure
        } else {
            dismiss()
        }
    }
}
