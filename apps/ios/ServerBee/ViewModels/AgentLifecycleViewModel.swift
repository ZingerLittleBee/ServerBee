import SwiftUI

/// Drives request-idempotent Server onboarding and Agent Authority actions.
@MainActor
@Observable
final class AgentLifecycleViewModel {
    var isWorking = false
    var errorMessage: String?

    /// The most recently minted code + its install command, for display.
    var issued: IssuedEnrollment?
    var onboardingReplay: OnboardingReplay?

    private(set) var onboardingRequestId = UUID().uuidString

    /// Newest released agent version (for the upgrade affordance). `nil` until loaded.
    var latestVersion: String?

    struct IssuedEnrollment: Identifiable {
        let id: String
        let code: String
        let expiresAt: String
        let installCommand: String
    }

    struct OnboardingReplay: Identifiable {
        var id: String { serverId }
        let serverId: String
        let outstandingOffer: OutstandingOffer?
    }

    // MARK: - Create

    /// Create a pending server and mint its first enrollment code.
    /// Returns the new server id on success.
    @discardableResult
    func createServer(name: String, serverUrl: String?, apiClient: APIClient) async -> String? {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let resp: CreateServerResponse = try await apiClient.post(
                "/api/servers",
                body: CreateServerRequest(
                    onboardingRequestId: onboardingRequestId,
                    name: name,
                    groupId: nil
                )
            )
            if let enrollment = resp.enrollment {
                issued = makeIssued(enrollment, serverUrl: serverUrl)
                onboardingReplay = nil
            } else {
                onboardingReplay = OnboardingReplay(
                    serverId: resp.serverId,
                    outstandingOffer: resp.outstandingOffer
                )
            }
            return resp.serverId
        } catch {
            errorMessage = message(for: error)
            return nil
        }
    }

    func resetOnboarding() {
        onboardingRequestId = UUID().uuidString
        onboardingReplay = nil
        issued = nil
        errorMessage = nil
    }

    // MARK: - Agent Authority

    func issueOffer(serverId: String, serverUrl: String?, apiClient: APIClient) async {
        await mint(
            path: "/api/servers/\(serverId)/agent-authority/offers",
            body: IssueOfferRequest(),
            serverUrl: serverUrl,
            apiClient: apiClient
        )
    }

    func replaceOffer(
        serverId: String,
        offerId: String,
        serverUrl: String?,
        apiClient: APIClient
    ) async {
        await mint(
            path: "/api/servers/\(serverId)/agent-authority/offers/\(offerId)/replace",
            body: IssueOfferRequest(),
            serverUrl: serverUrl,
            apiClient: apiClient
        )
    }

    func beginReenrollment(
        serverId: String,
        mode: ReenrollmentMode,
        serverUrl: String?,
        apiClient: APIClient
    ) async {
        await mint(
            path: "/api/servers/\(serverId)/agent-authority/re-enrollment",
            body: ReenrollmentRequest(mode: mode),
            serverUrl: serverUrl,
            apiClient: apiClient
        )
    }

    private func mint(path: String, body: any Encodable & Sendable, serverUrl: String?, apiClient: APIClient) async {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let resp: EnrollmentOfferResponse = try await apiClient.post(path, body: body)
            issued = makeIssued(resp.enrollment, serverUrl: serverUrl)
        } catch {
            errorMessage = message(for: error)
        }
    }

    // MARK: - Upgrade

    /// Fetch the newest released agent version (best-effort; failures are silent).
    func loadLatestVersion(apiClient: APIClient) async {
        if let resp: LatestAgentVersion = try? await apiClient.get("/api/agent/latest-version") {
            latestVersion = resp.version
        }
    }

    /// Trigger an agent self-upgrade. Returns nil on success, else an error.
    func upgrade(serverId: String, version: String, apiClient: APIClient) async -> String? {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let _: String = try await apiClient.post(
                "/api/servers/\(serverId)/upgrade",
                body: UpgradeRequest(version: version)
            )
            return nil
        } catch {
            let msg = message(for: error)
            errorMessage = msg
            return msg
        }
    }

    func revokeOffer(serverId: String, offerId: String, apiClient: APIClient) async -> Bool {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let _: RevokeOfferResponse = try await apiClient.delete(
                "/api/servers/\(serverId)/agent-authority/offers/\(offerId)"
            )
            return true
        } catch {
            errorMessage = message(for: error)
            return false
        }
    }

    func revokeAuthority(serverId: String, apiClient: APIClient) async -> Bool {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let _: RevokeAuthorityResponse = try await apiClient.delete(
                "/api/servers/\(serverId)/agent-authority"
            )
            return true
        } catch {
            errorMessage = message(for: error)
            return false
        }
    }

    // MARK: - Delete

    /// Delete a server. Returns true on success.
    func delete(serverId: String, apiClient: APIClient) async -> Bool {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let _: String = try await apiClient.delete("/api/servers/\(serverId)")
            return true
        } catch {
            errorMessage = message(for: error)
            return false
        }
    }

    // MARK: - Helpers

    /// Build the canonical agent install command for a minted code.
    static func installCommand(code: String, serverUrl: String?) -> String {
        let origin = (serverUrl ?? "").trimmingCharacters(in: .whitespaces)
        return "curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh "
            + "| sudo bash -s -- agent --server-url '\(origin)' --enrollment-code '\(code)'"
    }

    private func makeIssued(_ e: EnrollmentIssue, serverUrl: String?) -> IssuedEnrollment {
        IssuedEnrollment(
            id: e.id,
            code: e.code,
            expiresAt: e.expiresAt,
            installCommand: Self.installCommand(code: e.code, serverUrl: serverUrl)
        )
    }

    private func message(for error: Error) -> String {
        if case APIError.httpError(let code, let data) = error {
            if let msg = AccountSecurityViewModel.errorMessage(from: data) { return msg }
            switch code {
            case 403: return String(localized: "Admin permission required")
            case 404: return String(localized: "Server or enrollment offer not found")
            case 409: return String(localized: "An upgrade or enrollment is already in progress")
            default: break
            }
        }
        return String(localized: "Action failed")
    }
}
