import SwiftUI

/// Drives agent lifecycle actions (admin-only, server-enforced): create a
/// pending server, recover/regenerate its enrollment code, trigger an agent
/// upgrade, and delete a server. Each mint returns a one-time plaintext code.
@MainActor
@Observable
final class AgentLifecycleViewModel {
    var isWorking = false
    var errorMessage: String?

    /// The most recently minted code + its install command, for display.
    var issued: IssuedEnrollment?

    /// Newest released agent version (for the upgrade affordance). `nil` until loaded.
    var latestVersion: String?

    struct IssuedEnrollment: Identifiable {
        let id: String
        let code: String
        let expiresAt: String
        let installCommand: String
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
                body: CreateServerRequest(name: name, groupId: nil)
            )
            issued = makeIssued(resp.enrollment, serverUrl: serverUrl)
            return resp.serverId
        } catch {
            errorMessage = message(for: error)
            return nil
        }
    }

    // MARK: - Regenerate (pending server: mint a fresh code)

    func regenerateCode(serverId: String, serverUrl: String?, apiClient: APIClient) async {
        await mint(path: "/api/servers/\(serverId)/regenerate-code",
                   body: RegenerateCodeRequest(expectedEnrollmentId: nil),
                   serverUrl: serverUrl, apiClient: apiClient)
    }

    // MARK: - Recover (enrolled server: mint a new code, optionally revoke token)

    func recover(serverId: String, revokeImmediately: Bool, serverUrl: String?, apiClient: APIClient) async {
        await mint(path: "/api/servers/\(serverId)/recover",
                   body: RecoverRequest(revokeImmediately: revokeImmediately),
                   serverUrl: serverUrl, apiClient: apiClient)
    }

    private func mint(path: String, body: any Encodable & Sendable, serverUrl: String?, apiClient: APIClient) async {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let resp: EnrollmentOnlyResponse = try await apiClient.post(path, body: body)
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

    // MARK: - Revoke outstanding enrollment

    /// Revoke an outstanding (unconsumed) enrollment so a fresh recover can mint
    /// a new code. The `enrollmentId` is `ServerConfig.outstandingEnrollment.id`
    /// — NOT the server id. Returns true on success.
    func revokeEnrollment(enrollmentId: String, apiClient: APIClient) async -> Bool {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let _: String = try await apiClient.delete("/api/agent/enrollments/\(enrollmentId)")
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
            case 404: return String(localized: "Agent not connected")
            case 409: return String(localized: "An upgrade or enrollment is already in progress")
            default: break
            }
        }
        return String(localized: "Action failed")
    }
}
