import SwiftUI
import UIKit

struct LoginView: View {
    private enum Field: Hashable {
        case server
        case username
        case password
        case totp
    }

    @State private var viewModel = AuthViewModel()
    @State private var showQRScanner = false
    @State private var pairErrorMessage = ""
    @State private var isPairing = false
    @FocusState private var focusedField: Field?
    @Environment(AuthManager.self) private var authManager
    @ScaledMetric(relativeTo: .largeTitle) private var logoSize: CGFloat = 88

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(spacing: 16) {
                    // 28 + the stack's 16pt spacing gives the design's 44pt
                    // gap between the header and the field card.
                    header
                        .padding(.top, 56)
                        .padding(.bottom, 28)

                    if viewModel.step == .credentials {
                        credentialsCard
                        InsecureURLBanner(serverUrl: viewModel.serverUrlInput)
                    } else {
                        totpSection
                            .id("totp")
                    }

                    errorMessages

                    VStack(spacing: 12) {
                        loginButton
                        scanButton
                    }

                    Text("Scan the pairing code from the web dashboard to sign in without typing a password.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 24)

                    if viewModel.step == .totp {
                        Button {
                            viewModel.goBackToCredentials()
                        } label: {
                            Text("Back")
                                .frame(minWidth: 88, minHeight: 44)
                                .contentShape(Rectangle())
                        }
                        .foregroundStyle(.secondary)
                    }
                }
                .frame(maxWidth: 520)
                .padding(.horizontal, 16)
                .padding(.bottom, 24)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.interactively)
            .background(Color(.systemGroupedBackground))
            .onChange(of: viewModel.serverUrlInput) { clearErrors() }
            .onChange(of: viewModel.username) { clearErrors() }
            .onChange(of: viewModel.password) { clearErrors() }
            .onChange(of: viewModel.totpCode) { clearErrors() }
            .onAppear {
                // Signing out keeps the server URL so the user can log straight
                // back in to the same server.
                if viewModel.serverUrlInput.isEmpty, let saved = authManager.serverUrl {
                    viewModel.serverUrlInput = saved
                }
            }
            .onChange(of: viewModel.step) { _, newStep in
                if newStep == .totp {
                    // Defer one runloop so the totp field exists before we
                    // scroll to it on iPhone SE-class devices where the
                    // keyboard otherwise covers the field.
                    DispatchQueue.main.async {
                        withAnimation { proxy.scrollTo("totp", anchor: .center) }
                        focusedField = .totp
                    }
                }
            }
            .sheet(isPresented: $showQRScanner) {
                QRScannerView { serverUrl, code in
                    showQRScanner = false
                    Task { await runPair(serverUrl: serverUrl, code: code) }
                }
            }
        }
    }

    @MainActor
    private func runPair(serverUrl: String, code: String) async {
        isPairing = true
        pairErrorMessage = ""
        defer { isPairing = false }
        do {
            _ = try await viewModel.pair(serverUrl: serverUrl, code: code, authManager: authManager)
        } catch let error as AuthViewModel.PairError {
            pairErrorMessage = error.errorDescription ?? ""
        } catch {
            pairErrorMessage = String(localized: "Connection failed. Please check the server URL.")
        }
    }

    /// Every field the current step needs is filled in.
    private var canSubmit: Bool {
        switch viewModel.step {
        case .credentials:
            !viewModel.serverUrlInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                && !viewModel.username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                && !viewModel.password.isEmpty
        case .totp:
            !viewModel.totpCode.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }
    }

    /// A failed attempt's message stops applying once the user edits a field.
    private func clearErrors() {
        viewModel.errorMessage = ""
        pairErrorMessage = ""
    }

    /// Shared by the Log In button and the keyboard submit key, so both paths
    /// respect the same in-flight guards.
    private func submitLogin() {
        guard !viewModel.isLoading, !isPairing, canSubmit else { return }
        Task {
            await viewModel.login(authManager: authManager)
        }
    }
}

// MARK: - Subviews

private extension LoginView {
    var header: some View {
        VStack(spacing: 14) {
            Image("Logo")
                .resizable()
                .scaledToFit()
                .frame(width: min(logoSize, 132), height: min(logoSize, 132))
                .overlay(Circle().strokeBorder(Color.black.opacity(0.08), lineWidth: 0.5))
                .shadow(color: .black.opacity(0.10), radius: 15, y: 10)
                .accessibilityHidden(true)

            VStack(spacing: 4) {
                Text("ServerBee")
                    .font(.largeTitle.bold())
                    .accessibilityAddTraits(.isHeader)
                Text("Sign in to your server")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
    }

    var credentialsCard: some View {
        VStack(spacing: 0) {
            LoginFieldRow(label: String(localized: "Server")) {
                focusedField = .server
            } field: {
                TextField(
                    String(localized: "Server URL"),
                    text: $viewModel.serverUrlInput,
                    prompt: Text(verbatim: "https://your-server.com")
                )
                .keyboardType(.URL)
                .textContentType(.URL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.next)
                .focused($focusedField, equals: .server)
                .onSubmit { focusedField = .username }
            }

            LoginFieldDivider()

            LoginFieldRow(label: String(localized: "Username")) {
                focusedField = .username
            } field: {
                TextField(String(localized: "Username"), text: $viewModel.username, prompt: Text("Required"))
                    .textContentType(.username)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .submitLabel(.next)
                    .focused($focusedField, equals: .username)
                    .onSubmit { focusedField = .password }
            }

            LoginFieldDivider()

            LoginFieldRow(label: String(localized: "Password")) {
                focusedField = .password
            } field: {
                SecureField(String(localized: "Password"), text: $viewModel.password, prompt: Text("Required"))
                    .textContentType(.password)
                    .submitLabel(.go)
                    .focused($focusedField, equals: .password)
                    .onSubmit(submitLogin)
            }
        }
        .cardSurface(padding: 0)
    }

    var totpSection: some View {
        VStack(spacing: 12) {
            VStack(spacing: 4) {
                Text("Two-Factor Authentication")
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)
                Text("Enter the 6-digit code from your authenticator app")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity)

            LoginFieldRow(label: String(localized: "Code")) {
                focusedField = .totp
            } field: {
                TextField(String(localized: "Code"), text: $viewModel.totpCode, prompt: Text("000000"))
                    .keyboardType(.numberPad)
                    .textContentType(.oneTimeCode)
                    .font(.body.monospaced())
                    .focused($focusedField, equals: .totp)
                    .onSubmit(submitLogin)
            }
            .cardSurface(padding: 0)
        }
    }

    @ViewBuilder
    var errorMessages: some View {
        if !viewModel.errorMessage.isEmpty {
            errorText(viewModel.errorMessage)
        }

        if !pairErrorMessage.isEmpty {
            errorText(pairErrorMessage)
        }
    }

    func errorText(_ message: String) -> some View {
        Text(message)
            .font(.subheadline)
            .foregroundStyle(Color.serverOffline)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity)
    }

    var loginButton: some View {
        Button(action: submitLogin) {
            if viewModel.isLoading {
                ProgressView()
                    .tint(.white)
            } else {
                Text("Log In")
            }
        }
        .buttonStyle(LoginButtonStyle(prominent: true, isBusy: viewModel.isLoading))
        .disabled(viewModel.isLoading || isPairing || !canSubmit)
        .accessibilityLabel(Text("Log In"))
    }

    var scanButton: some View {
        Button {
            showQRScanner = true
        } label: {
            HStack(spacing: 8) {
                if isPairing {
                    ProgressView()
                        .tint(Color.accentColor)
                } else {
                    Image(systemName: "qrcode.viewfinder")
                        .accessibilityHidden(true)
                }
                Text("Scan QR Code")
            }
        }
        .buttonStyle(LoginButtonStyle(prominent: false, isBusy: isPairing))
        .disabled(isPairing)
    }
}

// MARK: - Field Row

/// One row of the grouped credentials card: a fixed-width leading label and a
/// trailing field. At accessibility text sizes the label stacks above the
/// field so neither is truncated. Like a native form row, a tap anywhere in
/// the row (label or padding) focuses the field.
private struct LoginFieldRow<FieldContent: View>: View {
    let label: String
    let onRowTap: () -> Void
    @ViewBuilder let field: FieldContent

    @ScaledMetric(relativeTo: .body) private var labelWidth: CGFloat = 92
    @ScaledMetric(relativeTo: .body) private var minRowHeight: CGFloat = 46
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        let stacked = dynamicTypeSize.isAccessibilitySize
        let layout = stacked
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 4))
            : AnyLayout(HStackLayout(spacing: 12))

        layout {
            Text(label)
                .frame(width: stacked ? nil : labelWidth, alignment: .leading)
                // Let label taps fall through to the row's tap target below.
                .allowsHitTesting(false)
                // The field below speaks this label instead.
                .accessibilityHidden(true)
            field
                .frame(maxWidth: .infinity, alignment: .leading)
                // A field with a prompt outside a Form exposes no label of its
                // own, so VoiceOver would only read the placeholder.
                .accessibilityLabel(Text(label))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .frame(minHeight: minRowHeight)
        .background {
            // Sits behind the field, so taps on the field itself still reach
            // the text input; only the label and padding land here.
            Color.clear
                .contentShape(Rectangle())
                .onTapGesture(perform: onRowTap)
                .accessibilityHidden(true)
        }
    }
}

/// Hairline separator inset 16pt from the leading edge, as in a grouped list.
private struct LoginFieldDivider: View {
    var body: some View {
        Divider()
            .padding(.leading, 16)
    }
}

// MARK: - Button Style

/// Full-width 50pt (Dynamic Type scaled) button: accent fill with white text
/// when prominent, accent text on a light accent tint otherwise. A busy button
/// keeps full opacity so its spinner stays legible; any other disabled button
/// is dimmed.
private struct LoginButtonStyle: ButtonStyle {
    let prominent: Bool
    let isBusy: Bool

    func makeBody(configuration: Configuration) -> some View {
        LoginButtonBody(configuration: configuration, prominent: prominent, isBusy: isBusy)
    }
}

private struct LoginButtonBody: View {
    let configuration: ButtonStyleConfiguration
    let prominent: Bool
    let isBusy: Bool

    @ScaledMetric(relativeTo: .headline) private var height: CGFloat = 50
    @Environment(\.isEnabled) private var isEnabled

    private var opacity: Double {
        if configuration.isPressed { return 0.7 }
        return isEnabled || isBusy ? 1 : 0.6
    }

    private var shape: RoundedRectangle {
        RoundedRectangle(cornerRadius: 14, style: .continuous)
    }

    var body: some View {
        configuration.label
            .font(.headline)
            .foregroundStyle(prominent ? Color.white : Color.accentColor)
            .multilineTextAlignment(.center)
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, minHeight: height)
            .background(prominent ? Color.accentColor : Color.accentColor.opacity(0.15), in: shape)
            .contentShape(shape)
            .opacity(opacity)
            .animation(.easeOut(duration: 0.15), value: configuration.isPressed)
    }
}
