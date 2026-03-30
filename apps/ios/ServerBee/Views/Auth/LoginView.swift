import SwiftUI
import UIKit

struct LoginView: View {
    @State private var viewModel = AuthViewModel()
    @State private var showQRScanner = false
    @State private var pairErrorMessage = ""
    @State private var isPairing = false
    @Environment(AuthManager.self) private var authManager

    var body: some View {
        ScrollView {
            VStack(spacing: 24) {
                VStack(spacing: 8) {
                    Image(systemName: "server.rack")
                        .font(.system(size: 60))
                        .foregroundStyle(Color.accentColor)
                    Text("ServerBee")
                        .font(.largeTitle.bold())
                }
                .padding(.top, 60)
                .padding(.bottom, 20)

                VStack(spacing: 16) {
                    if viewModel.step == .credentials {
                        credentialsFields
                    } else {
                        totpFields
                    }

                    if !viewModel.errorMessage.isEmpty {
                        Text(viewModel.errorMessage)
                            .font(.subheadline)
                            .foregroundStyle(.red)
                            .multilineTextAlignment(.center)
                    }

                    if !pairErrorMessage.isEmpty {
                        Text(pairErrorMessage)
                            .font(.subheadline)
                            .foregroundStyle(.red)
                            .multilineTextAlignment(.center)
                    }

                    Button {
                        Task {
                            await viewModel.login(authManager: authManager)
                        }
                    } label: {
                        Group {
                            if viewModel.isLoading {
                                ProgressView()
                                    .tint(.white)
                            } else {
                                Text("Login")
                                    .fontWeight(.semibold)
                            }
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 14)
                    }
                    .background(Color.accentColor)
                    .foregroundStyle(.white)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                    .disabled(viewModel.isLoading || isPairing)

                    Button {
                        showQRScanner = true
                    } label: {
                        HStack(spacing: 8) {
                            Image(systemName: "qrcode.viewfinder")
                            Text("Scan QR Code")
                                .fontWeight(.semibold)
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 14)
                    }
                    .background(Color(.systemGray5))
                    .foregroundStyle(Color.accentColor)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                    .disabled(isPairing)

                    if viewModel.step == .totp {
                        Button(String(localized: "Back")) {
                            viewModel.goBackToCredentials()
                        }
                        .foregroundStyle(.secondary)
                    }
                }
                .padding(.horizontal, 24)
            }
        }
        .scrollDismissesKeyboard(.interactively)
        .sheet(isPresented: $showQRScanner) {
            QRScannerView { serverUrl, code in
                showQRScanner = false
                Task {
                    await pair(serverUrl: serverUrl, code: code)
                }
            }
        }
    }

    // MARK: - QR Pairing

    @MainActor
    private func pair(serverUrl: String, code: String) async {
        isPairing = true
        pairErrorMessage = ""

        defer { isPairing = false }

        guard let url = URL(string: "\(serverUrl)/api/mobile/auth/pair") else {
            pairErrorMessage = String(localized: "Invalid server URL in QR code.")
            return
        }

        let body: [String: String] = [
            "code": code,
            "installation_id": InstallationID.getOrCreate(),
            "device_name": UIDevice.current.name,
        ]

        do {
            var request = URLRequest(url: url)
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONSerialization.data(withJSONObject: body)

            let (data, response) = try await URLSession.shared.data(for: request)

            guard let httpResponse = response as? HTTPURLResponse else {
                pairErrorMessage = String(localized: "Unexpected response from server.")
                return
            }

            guard httpResponse.statusCode == 200 else {
                switch httpResponse.statusCode {
                case 400:
                    pairErrorMessage = String(localized: "Invalid or expired QR code. Please try again.")
                case 404:
                    pairErrorMessage = String(localized: "Pairing endpoint not found. Check server version.")
                case 429:
                    pairErrorMessage = String(localized: "Too many attempts. Please try again later.")
                default:
                    pairErrorMessage = String(localized: "Pairing failed (HTTP \(httpResponse.statusCode)).")
                }
                return
            }

            let tokenResponse = try JSONDecoder.snakeCase.decode(
                ApiResponse<MobileTokenResponse>.self,
                from: data
            ).data

            authManager.setServerUrl(serverUrl)
            authManager.handleLoginResponse(tokenResponse)
        } catch {
            pairErrorMessage = String(localized: "Connection failed. Please check the server URL.")
        }
    }

    // MARK: - Subviews

    private var credentialsFields: some View {
        Group {
            LabeledField(label: String(localized: "Server URL")) {
                TextField("https://your-server.com", text: $viewModel.serverUrlInput)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            }

            LabeledField(label: String(localized: "Username")) {
                TextField(String(localized: "Username"), text: $viewModel.username)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            }

            LabeledField(label: String(localized: "Password")) {
                SecureField(String(localized: "Password"), text: $viewModel.password)
            }
        }
    }

    private var totpFields: some View {
        VStack(spacing: 12) {
            Text("Two-Factor Authentication")
                .font(.headline)
            Text("Enter the 6-digit code from your authenticator app")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

            TextField("000000", text: $viewModel.totpCode)
                .textFieldStyle(.roundedBorder)
                .keyboardType(.numberPad)
                .multilineTextAlignment(.center)
                .font(.title2.monospaced())
        }
    }
}

// MARK: - Labeled Field

private struct LabeledField<Content: View>: View {
    let label: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label)
                .font(.subheadline.weight(.medium))
            content
                .textFieldStyle(.roundedBorder)
        }
    }
}
