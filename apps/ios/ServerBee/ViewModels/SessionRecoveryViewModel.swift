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

    func recover(authManager: AuthManager) async {
        guard !isWorking, !username.isEmpty, !password.isEmpty,
              candidates.isEmpty || selectedSessionId != nil else { return }
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            candidates = try await authManager.recoverSession(username: username, password: password,
                totpCode: totpCode.isEmpty ? nil : totpCode, selectedSessionId: selectedSessionId)
            guard candidates.isEmpty else { return }
            password = ""
            totpCode = ""
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
