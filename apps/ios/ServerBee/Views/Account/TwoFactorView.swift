import SwiftUI

/// Two-factor (TOTP) management: shows current state, walks enrollment
/// (setup → scan/secret → confirm code), and disables with the account password.
struct TwoFactorView: View {
    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = AccountSecurityViewModel()
    @State private var showEnroll = false
    @State private var showDisable = false

    var body: some View {
        List {
            Section {
                HStack {
                    Label(String(localized: "Two-Factor Auth"), systemImage: "lock.shield")
                    Spacer()
                    if let enabled = viewModel.twoFactorEnabled {
                        Chip(text: enabled ? String(localized: "On") : String(localized: "Off"),
                             color: enabled ? .serverOnline : .secondary)
                    } else if viewModel.isLoadingStatus {
                        ProgressView()
                    }
                }
            } footer: {
                Text(String(localized: "Require a time-based one-time code from an authenticator app when signing in on the web."))
            }

            if viewModel.twoFactorEnabled == true {
                Section {
                    Button(role: .destructive) { showDisable = true } label: {
                        Label(String(localized: "Turn Off 2FA"), systemImage: "lock.open")
                    }
                }
            } else if viewModel.twoFactorEnabled == false {
                Section {
                    Button { showEnroll = true } label: {
                        Label(String(localized: "Set Up 2FA"), systemImage: "qrcode")
                    }
                }
            }
        }
        .navigationTitle(String(localized: "Two-Factor"))
        .navigationBarTitleDisplayMode(.inline)
        .task {
            await viewModel.loadStatus(apiClient: apiClient)
            #if DEBUG
            if viewModel.twoFactorEnabled == false, UITestSupport.autoPresent == "2fa-setup" { showEnroll = true }
            #endif
        }
        .sheet(isPresented: $showEnroll) {
            TwoFactorEnrollSheet(viewModel: viewModel)
        }
        .sheet(isPresented: $showDisable) {
            TwoFactorDisableSheet(viewModel: viewModel)
        }
    }
}

// MARK: - Enroll

private struct TwoFactorEnrollSheet: View {
    @Bindable var viewModel: AccountSecurityViewModel
    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss

    @State private var code = ""
    @State private var confirmError: String?

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 20) {
                    if viewModel.isEnrolling {
                        ProgressView().padding(.top, 60)
                    } else if let setup = viewModel.setup {
                        enrollContent(setup)
                    } else if let error = viewModel.enrollError {
                        ContentUnavailableView(
                            String(localized: "Setup failed"),
                            systemImage: "exclamationmark.triangle",
                            description: Text(error)
                        )
                        .padding(.top, 40)
                    }
                }
                .padding(16)
            }
            .background(Color(.systemGroupedBackground))
            .navigationTitle(String(localized: "Set Up 2FA"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(String(localized: "Cancel")) { dismiss() }
                }
            }
            .task {
                if viewModel.setup == nil { await viewModel.beginSetup(apiClient: apiClient) }
            }
        }
    }

    @ViewBuilder
    private func enrollContent(_ setup: TwoFactorSetup) -> some View {
        SectionCard(String(localized: "1. Scan in your authenticator"), systemImage: "qrcode") {
            VStack(spacing: 12) {
                if let image = Self.qrImage(from: setup.qrCodeBase64) {
                    image
                        .interpolation(.none)
                        .resizable()
                        .scaledToFit()
                        .frame(width: 200, height: 200)
                        .background(Color.white)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                }
                VStack(spacing: 4) {
                    Text(String(localized: "Or enter this key manually:"))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(setup.secret)
                        .font(.callout.monospaced())
                        .textSelection(.enabled)
                        .multilineTextAlignment(.center)
                }
            }
            .frame(maxWidth: .infinity)
        }

        SectionCard(String(localized: "2. Enter the 6-digit code"), systemImage: "number") {
            VStack(spacing: 10) {
                TextField(String(localized: "000000"), text: $code)
                    .keyboardType(.numberPad)
                    .textContentType(.oneTimeCode)
                    .font(.title2.monospaced())
                    .multilineTextAlignment(.center)
                    .padding(10)
                    .background(Color(.secondarySystemBackground))
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                if let confirmError {
                    Text(confirmError)
                        .font(.caption)
                        .foregroundStyle(Color.serverOffline)
                }
                Button {
                    Task { await confirm() }
                } label: {
                    if viewModel.isWorking {
                        ProgressView().frame(maxWidth: .infinity)
                    } else {
                        Text(String(localized: "Verify & Enable")).frame(maxWidth: .infinity)
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(code.count < 6 || viewModel.isWorking)
            }
        }
    }

    private func confirm() async {
        confirmError = nil
        if let failure = await viewModel.enable(code: code, apiClient: apiClient) {
            confirmError = failure
        } else {
            dismiss()
        }
    }

    /// Decode the server's base64 QR (raw base64 or a `data:` URI) into an Image.
    static func qrImage(from base64: String) -> Image? {
        var payload = base64
        if let range = payload.range(of: "base64,") {
            payload = String(payload[range.upperBound...])
        }
        guard let data = Data(base64Encoded: payload), let ui = UIImage(data: data) else { return nil }
        return Image(uiImage: ui)
    }
}

// MARK: - Disable

private struct TwoFactorDisableSheet: View {
    @Bindable var viewModel: AccountSecurityViewModel
    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss

    @State private var password = ""
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    SecureField(String(localized: "Account password"), text: $password)
                        .textContentType(.password)
                } footer: {
                    Text(String(localized: "Confirm your password to turn off two-factor authentication."))
                }
                if let error {
                    Section {
                        Label(error, systemImage: "exclamationmark.triangle.fill")
                            .foregroundStyle(Color.serverOffline)
                    }
                }
            }
            .navigationTitle(String(localized: "Turn Off 2FA"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(String(localized: "Cancel")) { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if viewModel.isWorking {
                        ProgressView()
                    } else {
                        Button(String(localized: "Disable"), role: .destructive) { Task { await disable() } }
                            .disabled(password.isEmpty)
                    }
                }
            }
        }
    }

    private func disable() async {
        error = nil
        if let failure = await viewModel.disable(password: password, apiClient: apiClient) {
            error = failure
        } else {
            dismiss()
        }
    }
}
