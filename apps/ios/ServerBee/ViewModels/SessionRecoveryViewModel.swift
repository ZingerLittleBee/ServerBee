import Foundation
import Observation

@MainActor
@Observable
final class SessionRecoveryViewModel {
    var username = ""
    var password = ""
    var totpCode = ""
    var selectedSessionId: String?
    private(set) var candidates: [MobileRecoveryCandidate] = []
    private(set) var requiresTOTP = false
    private(set) var isWorking = false
    private(set) var errorMessage: String?
    private var recoveryToken: String?

    var canConfirmQRSelection: Bool { recoveryToken != nil && selectedSessionId != nil && !isWorking }

    func recoverWithQRCode(serverUrl: String, code: String, authManager: AuthManager) async {
        guard !isWorking else { return }
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            // A highlighted candidate is not yet a confirmed cleanup target.
            // Already confirmed targets still come from durable auth storage.
            let result = try await authManager.recoverSession(serverUrl: serverUrl, pairingCode: code)
            acceptQRResult(result)
        } catch AuthError.staleIdentity {
            // A late scan cannot own a replacement login.
        } catch {
            handleQRError(error)
        }
    }

    func confirmQRSelection(authManager: AuthManager) async {
        guard canConfirmQRSelection, let recoveryToken, let selectedSessionId else { return }
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let result = try await authManager.recoverSession(recoveryToken: recoveryToken, selectedSessionId: selectedSessionId)
            acceptQRResult(result)
        } catch AuthError.staleIdentity {
            // The result belongs to the captured original identity only.
        } catch {
            handleQRError(error)
        }
    }

    func clearCredentials() {
        password = ""
        totpCode = ""
        recoveryToken = nil
    }

    private func acceptQRResult(_ result: MobileSessionRecoveryResponse) {
        candidates = result.candidates ?? []
        if result.outcome == .selectionRequired { selectedSessionId = nil }
        recoveryToken = result.recoveryToken
        password = ""
        totpCode = ""
        if candidates.isEmpty { clearCredentials() }
    }

    private func handleQRError(_ error: Error) {
        if case SessionRecoveryError.invalidRecoveryCode = error { recoveryToken = nil }
        errorMessage = error.localizedDescription
    }

    func recover(authManager: AuthManager) async {
        guard !isWorking, !username.isEmpty, !password.isEmpty,
              candidates.isEmpty || selectedSessionId != nil else { return }
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            recoveryToken = nil
            candidates = try await authManager.recoverSession(username: username, password: password,
                totpCode: totpCode.isEmpty ? nil : totpCode, selectedSessionId: selectedSessionId)
            guard candidates.isEmpty else { return }
            clearCredentials()
        } catch AuthError.twoFactorRequired {
            requiresTOTP = true
            errorMessage = AuthError.twoFactorRequired.localizedDescription
        } catch AuthError.staleIdentity {
            // Completion belongs to the original recovery screen only.
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func retryCleanup(authManager: AuthManager) async {
        guard !isWorking else { return }
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do { try await authManager.retrySessionCleanup() } catch AuthError.staleIdentity { /* A later identity owns the screen. */ } catch { errorMessage = error.localizedDescription }
    }
}
