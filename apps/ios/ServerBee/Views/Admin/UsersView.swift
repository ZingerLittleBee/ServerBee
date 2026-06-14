import SwiftUI

/// Admin user management: list, create, change role, reset password, delete.
/// Admin-only — the Settings entry is gated on role, and the server enforces it.
struct UsersView: View {
    @Environment(\.apiClient) private var apiClient
    @Environment(AuthManager.self) private var authManager
    @State private var viewModel = UsersViewModel()
    @State private var showCreate = false
    @State private var editing: AdminUser?

    private var currentUserId: String? { authManager.user?.id }

    var body: some View {
        List {
            if let error = viewModel.loadError {
                Section { Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline) }
            }
            if let error = viewModel.actionError {
                Section { Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline) }
            }
            ForEach(viewModel.users) { user in
                Button { editing = user } label: { userRow(user) }
                    .buttonStyle(.plain)
            }
        }
        .overlay { if viewModel.isLoading, viewModel.users.isEmpty { ProgressView() } }
        .navigationTitle(String(localized: "Users"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { showCreate = true } label: { Image(systemName: "person.badge.plus") }
            }
        }
        .task { await viewModel.load(apiClient: apiClient) }
        .refreshable { await viewModel.load(apiClient: apiClient) }
        .sheet(isPresented: $showCreate) { CreateUserSheet(viewModel: viewModel) }
        .sheet(item: $editing) { user in
            EditUserSheet(viewModel: viewModel, user: user, isSelf: user.id == currentUserId)
        }
    }

    private func userRow(_ user: AdminUser) -> some View {
        HStack(spacing: 12) {
            Image(systemName: user.isAdmin ? "person.fill.badge.plus" : "person.fill")
                .foregroundStyle(user.isAdmin ? Color.brandAccent : .secondary)
                .frame(width: 26)
            VStack(alignment: .leading, spacing: 3) {
                Text(user.username).font(.body).foregroundStyle(.primary)
                HStack(spacing: 6) {
                    Chip(text: user.role.capitalized, color: user.isAdmin ? .brandAccent : .secondary)
                    if user.has2fa { Chip(text: String(localized: "2FA"), systemImage: "lock.shield", color: .serverOnline) }
                }
            }
            Spacer()
            Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
        }
        .padding(.vertical, 2)
    }
}

// MARK: - Create

private struct CreateUserSheet: View {
    @Bindable var viewModel: UsersViewModel
    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss

    @State private var username = ""
    @State private var password = ""
    @State private var role = "member"
    @State private var error: String?
    @State private var working = false

    private var canSubmit: Bool {
        !username.trimmingCharacters(in: .whitespaces).isEmpty && password.count >= 8 && !working
    }

    var body: some View {
        NavigationStack {
            Form {
                Section(String(localized: "Account")) {
                    TextField(String(localized: "Username"), text: $username)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    SecureField(String(localized: "Password (min 8)"), text: $password)
                }
                Section(String(localized: "Role")) {
                    Picker(String(localized: "Role"), selection: $role) {
                        Text(String(localized: "Member")).tag("member")
                        Text(String(localized: "Admin")).tag("admin")
                    }
                    .pickerStyle(.segmented)
                }
                if let error {
                    Section { Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline) }
                }
            }
            .navigationTitle(String(localized: "New User"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button(String(localized: "Cancel")) { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    if working { ProgressView() } else {
                        Button(String(localized: "Create")) { Task { await create() } }.disabled(!canSubmit)
                    }
                }
            }
        }
    }

    private func create() async {
        working = true
        error = nil
        let failure = await viewModel.create(
            username: username.trimmingCharacters(in: .whitespaces),
            password: password,
            role: role,
            apiClient: apiClient
        )
        working = false
        if let failure { error = failure } else { dismiss() }
    }
}

// MARK: - Edit

private struct EditUserSheet: View {
    @Bindable var viewModel: UsersViewModel
    let user: AdminUser
    let isSelf: Bool
    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss

    @State private var role: String
    @State private var newPassword = ""
    @State private var error: String?
    @State private var working = false
    @State private var showDelete = false

    init(viewModel: UsersViewModel, user: AdminUser, isSelf: Bool) {
        self.viewModel = viewModel
        self.user = user
        self.isSelf = isSelf
        _role = State(initialValue: user.role)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section(String(localized: "Role")) {
                    Picker(String(localized: "Role"), selection: $role) {
                        Text(String(localized: "Member")).tag("member")
                        Text(String(localized: "Admin")).tag("admin")
                    }
                    .pickerStyle(.segmented)
                    if role != user.role {
                        Button(String(localized: "Save Role")) { Task { await saveRole() } }
                            .disabled(working)
                    }
                }

                Section {
                    SecureField(String(localized: "New password (min 8)"), text: $newPassword)
                    Button(String(localized: "Reset Password")) { Task { await resetPassword() } }
                        .disabled(newPassword.count < 8 || working)
                } header: {
                    Text(String(localized: "Reset password"))
                } footer: {
                    Text(String(localized: "Resetting signs the user out everywhere and revokes their API keys."))
                }

                if !isSelf {
                    Section {
                        Button(role: .destructive) { showDelete = true } label: {
                            Label(String(localized: "Delete User"), systemImage: "trash")
                        }
                    }
                }

                if let error {
                    Section { Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline) }
                }
            }
            .navigationTitle(user.username)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button(String(localized: "Done")) { dismiss() } }
            }
            .confirmationDialog(
                String(localized: "Delete \(user.username)?"),
                isPresented: $showDelete,
                titleVisibility: .visible
            ) {
                Button(String(localized: "Delete"), role: .destructive) { Task { await delete() } }
                Button(String(localized: "Cancel"), role: .cancel) {}
            } message: {
                Text(String(localized: "This permanently removes the account and all its sessions and keys."))
            }
        }
    }

    private func saveRole() async {
        working = true; error = nil
        let failure = await viewModel.setRole(id: user.id, role: role, apiClient: apiClient)
        working = false
        if let failure { error = failure }
    }

    private func resetPassword() async {
        working = true; error = nil
        let failure = await viewModel.resetPassword(id: user.id, password: newPassword, apiClient: apiClient)
        working = false
        if let failure { error = failure } else { newPassword = "" }
    }

    private func delete() async {
        await viewModel.delete(id: user.id, apiClient: apiClient)
        if viewModel.actionError == nil { dismiss() }
    }
}
