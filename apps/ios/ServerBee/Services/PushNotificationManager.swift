import Foundation
import UIKit
import UserNotifications

/// Protocol abstraction so tests can inject a spy.
@MainActor
protocol PushNotificationManaging: AnyObject {
    var permissionGranted: Bool { get }
    var deviceToken: String? { get }

    func configure(apiClient: APIClient)
    func requestPermission() async

    nonisolated func didRegisterForRemoteNotifications(deviceToken data: Data)
    nonisolated func didFailToRegisterForRemoteNotifications(error: Error)

    /// Parse a push payload and return a deep link (or nil if not actionable).
    nonisolated func handleNotificationResponse(_ response: UNNotificationResponse) -> ServerDeepLink?

    /// Unregister the device token from the server. Must NOT throw — failures
    /// are logged. Local auth must still clear even if the server call fails.
    func unregister() async
    func unregister(context: MobileAuthenticationContext?) async
}

extension PushNotificationManaging {
    func unregister(context: MobileAuthenticationContext?) async {
        await unregister()
    }
}

@MainActor
@Observable
final class PushNotificationManager: NSObject, PushNotificationManaging {
    private(set) var permissionGranted = false
    private(set) var authorizationStatus: UNAuthorizationStatus = .notDetermined
    private(set) var deviceToken: String?
    private(set) var confirmed: PushSetup?
    private(set) var isSaving = false
    private(set) var errorMessage: String?
    private(set) var verificationUnavailable = false

    private let system: any PushSystemBoundary
    private let relay: any PushRelayBoundary
    private var apiClient: APIClient?
    private var context: MobileAuthenticationContext?
    private var acceptingRegistrations = false
    private var uploads: [UUID: (generation: UUID, task: Task<Void, Never>)] = [:]
    private var grants: [UUID: (grant: RelayGrant, url: String)] = [:]
    private var uploadedToken: String?

    init(system: any PushSystemBoundary = NativePushSystem(), relay: any PushRelayBoundary = AppAttestPushRelay()) {
        self.system = system
        self.relay = relay
        super.init()
    }

    func configure(apiClient: APIClient) {
        let next = apiClient.captureContext()
        if context?.generation != next?.generation {
            confirmed = nil
            errorMessage = nil
            uploadedToken = nil
            isSaving = false
        }
        self.apiClient = apiClient
        context = next
        acceptingRegistrations = next != nil
    }

    /// Read permission without prompting. Launch, foreground and connectivity
    /// recovery all use this path, and only reconcile confirmed opt-in intent.
    func reconcile() async {
        guard let apiClient, let captured = context, apiClient.isCurrent(captured) else { return }
        do {
            let status = await system.authorization()
            let setup: PushSetup = try await apiClient.get("/api/mobile/push/settings", context: captured)
            guard apiClient.isCurrent(captured), context?.generation == captured.generation else { return }
            authorizationStatus = status
            permissionGranted = status == .authorized || status == .provisional || status == .ephemeral
            confirmed = setup
            verificationUnavailable = !relay.supported
            errorMessage = nil
            if setup.preferences.enabled && permissionGranted && relay.supported {
                system.register()
                let remaining = setup.grantExpiresAt.flatMap { ISO8601DateFormatter.shared.date(from: $0) }?.timeIntervalSinceNow ?? 0
                let needsRenewal = !setup.registered || remaining < 3600
                if let deviceToken, needsRenewal { uploadToken(deviceToken, renew: true) }
            }
        } catch { report(error, captured: captured) }
    }

    /// Save intent first. The view reflects only the returned Server state.
    func savePreferences(_ preferences: PushPreferences) async {
        guard !isSaving, let apiClient, let captured = context, let confirmed else { return }
        isSaving = true
        defer { if context?.generation == captured.generation { isSaving = !uploads.isEmpty } }
        do {
            let setup: PushSetup = try await apiClient.send(
                "/api/mobile/push/settings", method: "PUT",
                body: PushPreferencesRequest(expectedRevision: confirmed.revision, preferences: preferences), context: captured
            )
            guard apiClient.isCurrent(captured), context?.generation == captured.generation else { return }
            self.confirmed = setup
            errorMessage = nil
            if preferences.enabled {
                verificationUnavailable = !relay.supported
                if relay.supported && !confirmed.preferences.enabled { await requestPermission() }
            } else if let binding = grants.removeValue(forKey: captured.generation) {
                // Server opt-out is already durable even if Relay cleanup fails.
                do { try await relay.revoke(binding.grant, relayUrl: binding.url) }
                catch { report(error, captured: captured) }
            }
        } catch { report(error, captured: captured) }
    }

    func requestPermission() async {
        guard confirmed?.preferences.enabled == true, acceptingRegistrations,
              let apiClient, let captured = context, apiClient.isCurrent(captured), relay.supported else { return }
        do {
            let granted = try await system.requestPermission()
            guard apiClient.isCurrent(captured), context?.generation == captured.generation else { return }
            permissionGranted = granted
            authorizationStatus = await system.authorization()
            guard apiClient.isCurrent(captured), context?.generation == captured.generation else { return }
            if granted { system.register() }
            else { errorMessage = String(localized: "Notification permission is disabled. Open system settings to enable it.") }
            if granted, let deviceToken { uploadToken(deviceToken) }
        } catch { report(error, captured: captured) }
    }

    func waitForPendingRegistrations() async {
        let generation = context?.generation
        let tasks = uploads.values.filter { $0.generation == generation }.map { $0.task }
        for task in tasks { await task.value }
    }

    func retry() async {
        await waitForPendingRegistrations()
        await reconcile()
        if let deviceToken, confirmed?.preferences.enabled == true, permissionGranted, uploads.isEmpty { uploadToken(deviceToken, renew: true) }
    }

    nonisolated func didRegisterForRemoteNotifications(deviceToken data: Data) {
        let token = data.map { String(format: "%02x", $0) }.joined()
        Task { @MainActor in
            // APNs tokens belong to the installation. Retain early callbacks,
            // but upload only after the current login confirms explicit opt-in.
            self.deviceToken = token
            if self.confirmed?.preferences.enabled == true { self.uploadToken(token) }
        }
    }

    nonisolated func didFailToRegisterForRemoteNotifications(error: Error) {
        Task { @MainActor in self.errorMessage = String(localized: "APNs registration failed. Retry notification setup.") }
    }

    private func uploadToken(_ token: String, renew: Bool = false) {
        guard acceptingRegistrations, permissionGranted, relay.supported, let apiClient, let captured = context,
              let setup = confirmed, setup.preferences.enabled, apiClient.isCurrent(captured) else { return }
        // An unchanged callback need not renew a still-valid registration.
        if !renew && token == uploadedToken && setup.registered { return }
        guard !uploads.values.contains(where: { $0.generation == captured.generation }) else { return }
        let id = UUID()
        isSaving = true
        let upload = Task { @MainActor in
            defer {
                self.uploads[id] = nil
                if self.context?.generation == captured.generation {
                    self.isSaving = false
                    if let latest = self.deviceToken, latest != token { self.uploadToken(latest) }
                }
            }
            do {
                let grant = try await self.relay.register(token: token, relayUrl: setup.relayUrl)
                // Keep the original identity's grant available to logout even
                // if the Server response or a subsequent identity check fails.
                self.grants[captured.generation] = (grant, setup.relayUrl)
                guard apiClient.isCurrent(captured), self.context?.generation == captured.generation else { throw AuthError.staleIdentity }
                let result: PushSetup = try await apiClient.send(
                    "/api/mobile/push/verified-register", method: "POST",
                    body: VerifiedPushRequest(expectedRevision: setup.revision, deviceToken: token, environment: grant.environment,
                                              keyId: grant.keyId, grantId: grant.grantId, grantToken: grant.grantToken), context: captured
                )
                guard apiClient.isCurrent(captured), self.context?.generation == captured.generation else { throw AuthError.staleIdentity }
                self.confirmed = result
                self.uploadedToken = token
                self.errorMessage = nil
            } catch { self.report(error, captured: captured) }
        }
        uploads[id] = (captured.generation, upload)
    }

    private func report(_ error: Error, captured: MobileAuthenticationContext) {
        guard let apiClient, apiClient.isCurrent(captured), context?.generation == captured.generation else { return }
        if case AuthError.staleIdentity = error { return }
        errorMessage = AccountSecurityViewModel.message(for: error, fallback: String(localized: "Notification setup failed. Retry to confirm registration."))
    }

    func unregister() async { await unregister(context: context) }

    func unregister(context capturedContext: MobileAuthenticationContext?) async {
        let capturedClient = apiClient
        if context?.generation == capturedContext?.generation { acceptingRegistrations = false }
        let pending = uploads.values.filter { $0.generation == capturedContext?.generation }.map { $0.task }
        for upload in pending { await upload.value }
        if let capturedClient, let capturedContext {
            do { try await capturedClient.postCleanup("/api/mobile/push/unregister", context: capturedContext) }
            catch { AppLog.push.error("Push unregister failed; session revocation follows") }
            if let binding = grants.removeValue(forKey: capturedContext.generation) {
                do { try await relay.revoke(binding.grant, relayUrl: binding.url) }
                catch { AppLog.push.error("Relay revocation failed; Server session revocation still stops delivery") }
            }
        }
        if context?.generation == capturedContext?.generation {
            context = nil
            confirmed = nil
            deviceToken = nil
            uploadedToken = nil
            isSaving = false
        }
    }

    /// Parse a notification tap into a deep link.
    /// Backend payload (see `crates/server/src/service/apns.rs`) attaches
    /// `server_id` and optionally `rule_id` as APNs custom data.
    nonisolated func handleNotificationResponse(_ response: UNNotificationResponse) -> ServerDeepLink? {
        let userInfo = response.notification.request.content.userInfo
        if let serverId = userInfo["server_id"] as? String, !serverId.isEmpty {
            return .serverDetail(serverId: serverId)
        }
        if let ruleId = userInfo["rule_id"] as? String, !ruleId.isEmpty {
            return .alertDetail(alertKey: ruleId)
        }
        return nil
    }
}
