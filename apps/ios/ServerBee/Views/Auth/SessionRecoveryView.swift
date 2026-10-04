import SwiftUI

struct SessionRecoveryView: View {
    private enum Field: Hashable { case username, password, totp }

    @Environment(AuthManager.self) private var authManager
    @Environment(\.privacyMode) private var privacyMode
    @State private var viewModel: SessionRecoveryViewModel
    @State private var showPasswordRecovery: Bool
    @State private var confirmSelection = false
    @State private var confirmationUsesQR = true
    @State private var showQRScanner = false
    @FocusState private var focusedField: Field?

    init(viewModel: SessionRecoveryViewModel = SessionRecoveryViewModel(), passwordExpanded: Bool = false) {
        _viewModel = State(initialValue: viewModel)
        _showPasswordRecovery = State(initialValue: passwordExpanded)
    }

    var body: some View {
        NavigationStack {
            ScrollViewReader { proxy in
                ScrollView {
                    VStack(spacing: 24) {
                        AuthenticationHeader(title: "Recover session",
                            subtitle: "Verify your original account to finish signing out.", compact: true)
                            .padding(.top, 24)
                            .padding(.bottom, 8)
                        if let identity = authManager.sessionRecovery {
                            identityCard(identity)
                            InsecureURLBanner(serverUrl: identity.serverUrl)
                            if let error = recoveryError { errorCard(error) }
                            if !viewModel.candidates.isEmpty { selectionSection }
                            primaryAction
                            recoveryInstructions
                            passwordCard
                                .id("passwordRecovery")
                            retryAction
                            Text("Only the original session is cleaned up. Your saved identity stays on this device until the server confirms cleanup.")
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                                .multilineTextAlignment(.center)
                                .padding(.horizontal, 12)
                        }
                    }
                    .frame(maxWidth: 520)
                    .padding(.horizontal, 20)
                    .padding(.bottom, 28)
                    .frame(maxWidth: .infinity)
                }
                .scrollDismissesKeyboard(.interactively)
                .background(Color(.systemGroupedBackground))
                .onChange(of: viewModel.requiresTOTP) { _, required in
                    guard required else { return }
                    showPasswordRecovery = true
                    DispatchQueue.main.async {
                        proxy.scrollTo("passwordRecovery", anchor: .bottom)
                        focusedField = .totp
                    }
                }
            }
            .disabled(viewModel.isWorking)
            .toolbar(.hidden, for: .navigationBar)
            .task(id: authManager.sessionRecovery?.loginId) {
                viewModel.username = authManager.sessionRecovery?.username ?? ""
                viewModel.clearCredentials()
            }
            .onDisappear { viewModel.clearCredentials() }
            .sheet(isPresented: $showQRScanner) {
                QRScannerView { serverUrl, code in
                    showQRScanner = false
                    confirmationUsesQR = true
                    Task { await viewModel.recoverWithQRCode(serverUrl: serverUrl, code: code, authManager: authManager) }
                }
            }
            .confirmationDialog("Sign out selected session?", isPresented: $confirmSelection, titleVisibility: .visible) {
                Button("Sign Out", role: .destructive) {
                    Task {
                        if confirmationUsesQR { await viewModel.confirmQRSelection(authManager: authManager) } else {
                            await viewModel.recover(authManager: authManager)
                        }
                    }
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Only the selected session will be signed out. Other sessions stay signed in.")
            }
        }
    }
}

private extension SessionRecoveryView {
    var recoveryError: String? {
        viewModel.errorMessage ?? (viewModel.candidates.isEmpty ? authManager.recoveryError : nil)
    }

    var canSubmitPassword: Bool {
        !viewModel.isWorking && !viewModel.username.isEmpty && !viewModel.password.isEmpty
            && (!viewModel.requiresTOTP || !viewModel.totpCode.isEmpty)
            && (viewModel.candidates.isEmpty || viewModel.selectedSessionId != nil)
    }

    func identityCard(_ identity: SessionRecoveryIdentity) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 12) {
                IconTile(systemImage: "person.fill", color: .accentColor)
                VStack(alignment: .leading, spacing: 3) {
                    Text("Saved session").font(.caption).foregroundStyle(.secondary)
                    Text(verbatim: identity.username).font(.headline).textSelection(.enabled)
                }
                Spacer(minLength: 0)
            }
            Divider()
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "server.rack")
                    .font(.system(size: 20))
                    .foregroundStyle(.secondary)
                    .frame(width: 30)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 3) {
                    Text("Server").font(.caption).foregroundStyle(.secondary)
                    Text(verbatim: identity.serverUrl.maskingIPs(privacyMode))
                        .font(.subheadline)
                        .fixedSize(horizontal: false, vertical: true)
                        .textSelection(.enabled)
                }
            }
            if let selected = identity.selectedSessionId {
                Label("Selected session", systemImage: "checkmark.circle.fill")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(Color.accentColor)
                Text(verbatim: selected).font(.caption2.monospaced()).foregroundStyle(.secondary).textSelection(.enabled)
            }
        }
        .cardSurface(padding: 16)
    }

    func errorCard(_ message: String) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "exclamationmark.circle.fill")
                .foregroundStyle(Color.serverOffline)
                .accessibilityHidden(true)
            Text(verbatim: message).font(.subheadline).fixedSize(horizontal: false, vertical: true)
        }
        .cardSurface(padding: 16)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("recovery.error")
    }

    var primaryAction: some View {
        VStack(spacing: 12) {
            if viewModel.candidates.isEmpty {
                Button { showQRScanner = true } label: {
                    actionLabel("Scan QR Code to recover session", systemImage: "qrcode.viewfinder")
                }
                .buttonStyle(AuthenticationButtonStyle(prominent: true, isBusy: viewModel.isWorking))
                .accessibilityIdentifier("recovery.scan")
            } else {
                Button { confirmSelection = true } label: {
                    actionLabel("Sign out selected session", systemImage: "rectangle.portrait.and.arrow.right")
                }
                .buttonStyle(AuthenticationButtonStyle(prominent: true, isBusy: viewModel.isWorking))
                .disabled(confirmationUsesQR ? !viewModel.canConfirmQRSelection : !canSubmitPassword)
                .accessibilityIdentifier("recovery.confirm")
                Button { showQRScanner = true } label: {
                    Label("Scan a new QR code", systemImage: "qrcode.viewfinder")
                        .frame(minHeight: 44)
                        .frame(maxWidth: .infinity)
                        .contentShape(Rectangle())
                }
                .font(.subheadline.weight(.semibold))
                .accessibilityIdentifier("recovery.rescan")
            }
        }
    }

    func actionLabel(_ title: LocalizedStringKey, systemImage: String) -> some View {
        HStack(spacing: 8) {
            if viewModel.isWorking { ProgressView().tint(.white) } else {
                Image(systemName: systemImage).accessibilityHidden(true)
            }
            Text(title)
        }
    }

    var recoveryInstructions: some View {
        VStack(alignment: .leading, spacing: 16) {
            instruction("Get a recovery code", systemImage: "desktopcomputer",
                message: "Generate a pairing code from Settings → Mobile Devices → Add Device on the original account's web dashboard.")
            instruction("Use a new code to sign in", systemImage: "arrow.clockwise",
                message: "After recovery, close the QR dialog and add a device again to get a new sign-in code.")
        }
        .padding(.horizontal, 4)
    }

    func instruction(_ title: LocalizedStringKey, systemImage: String, message: LocalizedStringKey) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: systemImage)
                .font(.system(size: 16))
                .foregroundStyle(.secondary)
                .frame(width: 24)
                .padding(.top, 2)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.subheadline.weight(.semibold))
                Text(message).font(.footnote).foregroundStyle(.secondary)
            }
            .fixedSize(horizontal: false, vertical: true)
        }
        .accessibilityElement(children: .combine)
    }
}

private extension SessionRecoveryView {
    var passwordCard: some View {
        DisclosureGroup(isExpanded: $showPasswordRecovery) {
            VStack(spacing: 0) {
                AuthenticationFieldRow(label: String(localized: "Username")) { focusedField = .username } field: {
                    TextField("Username", text: $viewModel.username)
                        .textContentType(.username)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .focused($focusedField, equals: .username)
                        .submitLabel(.next)
                        .onSubmit { focusedField = .password }
                }
                AuthenticationFieldDivider()
                AuthenticationFieldRow(label: String(localized: "Password")) { focusedField = .password } field: {
                    SecureField("Password", text: $viewModel.password)
                        .textContentType(.password)
                        .focused($focusedField, equals: .password)
                        .submitLabel(viewModel.requiresTOTP ? .next : .go)
                        .onSubmit {
                            if viewModel.requiresTOTP { focusedField = .totp } else { submitPassword() }
                        }
                }
                if viewModel.requiresTOTP {
                    AuthenticationFieldDivider()
                    AuthenticationFieldRow(label: String(localized: "Verification code")) { focusedField = .totp } field: {
                        TextField("Verification code", text: $viewModel.totpCode)
                            .textContentType(.oneTimeCode)
                            .keyboardType(.numberPad)
                            .focused($focusedField, equals: .totp)
                            .onSubmit(submitPassword)
                    }
                }
            }
            .padding(.horizontal, -16)
            Button(action: submitPassword) {
                if viewModel.candidates.isEmpty {
                    Text("Verify account and finish sign-out")
                } else {
                    Text("Sign out selected session")
                }
            }
            .buttonStyle(AuthenticationButtonStyle(prominent: false, isBusy: viewModel.isWorking))
            .disabled(!canSubmitPassword)
            .padding(.top, 8)
        } label: {
            Label("Use account password instead", systemImage: "key.horizontal")
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.primary)
        }
        .cardSurface(padding: 16)
    }

    func submitPassword() {
        guard canSubmitPassword else { return }
        confirmationUsesQR = false
        if viewModel.candidates.isEmpty {
            Task { await viewModel.recover(authManager: authManager) }
        } else { confirmSelection = true }
    }

    var retryAction: some View {
        Button { Task { await viewModel.retryCleanup(authManager: authManager) } } label: {
            Label("Retry secure sign-out", systemImage: "arrow.clockwise")
        }
        .buttonStyle(AuthenticationButtonStyle(prominent: false, isBusy: viewModel.isWorking))
        .accessibilityIdentifier("recovery.retry")
    }

    var selectionSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Choose the original session")
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
            Text("The saved credentials no longer identify a session. Choose the original session by its device name and dates. Nothing is selected automatically.")
                .font(.footnote)
                .foregroundStyle(.secondary)
            VStack(spacing: 0) {
                ForEach(Array(viewModel.candidates.enumerated()), id: \.element.id) { index, candidate in
                    if index > 0 { Divider().padding(.leading, 16) }
                    RecoverySessionRow(candidate: candidate, selected: viewModel.selectedSessionId == candidate.mobileSessionId) {
                        viewModel.selectedSessionId = candidate.mobileSessionId
                    }
                    .disabled(authManager.sessionRecovery?.selectedSessionId != nil)
                }
            }
            .cardSurface(padding: 0)
        }
    }
}

private struct RecoverySessionRow: View {
    let candidate: MobileRecoveryCandidate
    let selected: Bool
    let select: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Button(action: select) {
                HStack(alignment: .top, spacing: 12) {
                    Image(systemName: "iphone")
                        .font(.system(size: 20))
                        .foregroundStyle(.secondary)
                        .frame(width: 24)
                        .accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(verbatim: candidate.deviceName).font(.headline).foregroundStyle(.primary)
                        Text(String(localized: "Created \(createdTime(candidate.createdAt))")).foregroundStyle(.secondary)
                        Text(String(localized: "Last used \(Formatters.formatRelativeTime(candidate.lastUsedAt))")).foregroundStyle(.secondary)
                    }
                    .font(.footnote)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                        .font(.title3)
                        .foregroundStyle(selected ? Color.accentColor : .secondary)
                        .accessibilityHidden(true)
                }
                .frame(minHeight: 44)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .combine)
            .accessibilityAddTraits(selected ? [.isSelected] : [])
            DisclosureGroup("Session details") {
                Text(verbatim: candidate.mobileSessionId)
                    .font(.caption2.monospaced())
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            .font(.caption)
            .padding(.leading, 36)
        }
        .padding(16)
        .background(selected ? Color.accentColor.opacity(0.08) : .clear)
    }

    private func createdTime(_ value: String) -> String {
        ISO8601DateFormatter.shared.date(from: value)?.formatted(date: .abbreviated, time: .shortened) ?? value
    }
}
