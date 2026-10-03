import CryptoKit
import Foundation
import UIKit
import UserNotifications

@MainActor
@Observable
final class PushNotificationManager: NSObject {
    private(set) var permissionGranted = false
    private(set) var authorizationStatus: UNAuthorizationStatus = .notDetermined
    private(set) var deviceToken: String?
    private(set) var confirmed: PushSetup?
    private(set) var isSaving = false
    var errorMessage: String? { failedPreferences?.message ?? registrationErrorMessage ?? testDelivery.errorMessage }
    var unconfirmedPreferences: PushPreferences? { failedPreferences?.preferences }
    private(set) var verificationUnavailable = false
    var testResult: TestPushResponse? { testDelivery.result }
    var isTesting: Bool { testDelivery.isTesting }
    private let testDelivery: PushTestDelivery
    private let system: any PushSystemBoundary
    private let relay: any PushRelayBoundary
    private let storage: any PushSetupStorage
    private var apiClient: APIClient?
    private var context: MobileAuthenticationContext?
    private var acceptingRegistrations = false
    private var uploads: [UUID: (generation: UUID, task: Task<Void, Never>)] = [:]
    private var grants: [UUID: (grant: RelayGrant, url: String)] = [:]
    private var uploadedToken: String?
    private var registrationErrorMessage: String?
    private var failedPreferences: FailedPreferenceSave?
    // One write/permission operation owns setup at a time. Each read owns a
    // unique token, invalidated by writes and superseded by newer reads even
    // when Relay inspection changes without a registration revision change.
    private var latestRead: UUID?
    private var activeWrite: UUID?
    private var permissionRequest: UUID?

    init(
        system: any PushSystemBoundary = NativePushSystem(), relay: any PushRelayBoundary = AppAttestPushRelay(),
        storage: any PushSetupStorage = KeychainPushSetupStorage()
    ) {
        self.system = system
        self.relay = relay
        self.storage = storage
        testDelivery = PushTestDelivery(storage: storage)
        super.init()
    }

    /// Read permission without prompting. Launch, foreground and connectivity
    /// recovery all use this path, and only reconcile confirmed opt-in intent.
    func reconcile() async {
        guard let captured = context else { return }
        _ = await reconcileSetup(context: captured)
    }

    private func reconcileSetup(context captured: MobileAuthenticationContext) async -> Bool {
        guard !isSaving, acceptingRegistrations, let apiClient, context?.generation == captured.generation,
              apiClient.isCurrent(captured) else { return false }
        let read = UUID()
        latestRead = read
        do {
            let status = await system.authorization()
            var setup: PushSetup = try await apiClient.get("/api/mobile/push/settings", context: captured)
            guard ownsRead(read, captured: captured),
                  setup.revision >= (confirmed?.revision ?? 0) else { return false }
            authorizationStatus = status
            permissionGranted = status == .authorized || status == .provisional || status == .ephemeral
            let pending = pendingGrant(captured, url: setup.relayUrl)
            if pending != nil { setup.registered = false }
            confirmed = setup
            confirmPreferenceSave(setup, captured: captured)
            verificationUnavailable = !relay.supported
            if pending != nil {
                registrationErrorMessage = String(localized: "Notification setup failed. Retry to confirm registration.")
            } else { registrationErrorMessage = nil }
            if setup.preferences.enabled && permissionGranted && relay.supported {
                system.register()
                let remaining = setup.grantExpiresAt.flatMap { ISO8601DateFormatter.shared.date(from: $0) }?.timeIntervalSinceNow ?? 0
                let needsRenewal = !setup.registered || remaining < 3600
                if let deviceToken, needsRenewal { uploadToken(deviceToken, renew: true) }
            }
            return true
        } catch {
            if ownsRead(read, captured: captured) { report(error, captured: captured) }
            return false
        }
    }

    /// Save intent first. The view reflects only the returned Server state.
    func savePreferences(_ preferences: PushPreferences) async {
        guard !isSaving, acceptingRegistrations, let apiClient, let captured = context, let confirmed else { return }
        // The Server's current role confirmation takes precedence over cached
        // login metadata and hidden draft categories after an administrator demotion.
        var permitted = preferences
        if !confirmed.securityAllowed { permitted.security = false }
        if confirmed.tasksAllowed != true { permitted.taskFailure = false; permitted.taskSuccess = false }
        let write = UUID()
        beginWrite(write)
        defer { finishWrite(write) }
        do {
            let setup: PushSetup = try await apiClient.send(
                "/api/mobile/push/settings", method: "PUT",
                body: PushPreferencesRequest(expectedRevision: confirmed.revision, preferences: permitted), context: captured
            )
            guard ownsWrite(write, captured: captured) else { return }
            guard setup.revision >= (self.confirmed?.revision ?? 0) else { throw PushSetupError.unavailable }
            self.confirmed = setup
            failedPreferences = nil
            if pendingGrant(captured, url: setup.relayUrl) != nil, preferences.enabled {
                self.confirmed?.registered = false
            } else { registrationErrorMessage = nil }
            // Permission and grant cleanup follow the confirmed PUT. Releasing
            // this write first lets permission completion start its own upload.
            finishWrite(write)
            if setup.preferences.enabled {
                verificationUnavailable = !relay.supported
                if relay.supported && !confirmed.preferences.enabled {
                    await requestPermission(context: captured)
                } else if permissionGranted, let deviceToken { uploadToken(deviceToken) }
            } else {
                clearContentKey(captured)
                let pending = pendingGrant(captured, url: setup.relayUrl)
                clearPending(captured)
                let binding = grants.removeValue(forKey: captured.generation)
                if let grant = binding?.grant ?? pending {
                    do { try await relay.revoke(grant, relayUrl: binding?.url ?? setup.relayUrl) } catch {
                        report(error, captured: captured)
                    }
                }
            }

        } catch {
            guard ownsWrite(write, captured: captured) else { return }
            if case AuthError.staleIdentity = error { return }
            failedPreferences = FailedPreferenceSave(
                generation: captured.generation, expectedRevision: confirmed.revision, preferences: permitted,
                message: AccountSecurityViewModel.message(for: error, fallback: String(localized: "Notification setup failed. Retry to confirm registration."))
            )
        }
    }
    func requestPermission() async {
        guard let captured = context else { return }
        await requestPermission(context: captured)
    }
    func waitForPendingRegistrations() async {
        await waitForPendingRegistrations(generation: context?.generation)
    }
    func retry() async {
        guard let apiClient, let captured = context, apiClient.isCurrent(captured) else { return }
        await waitForPendingRegistrations(generation: captured.generation)
        guard apiClient.isCurrent(captured), context?.generation == captured.generation else { return }
        guard await reconcileSetup(context: captured), apiClient.isCurrent(captured), context?.generation == captured.generation,
              confirmed?.preferences.enabled == true else { return }
        // Only an explicit user retry can continue a saved opt-in that was
        // interrupted before permission. Ordinary reconciliation never prompts.
        if authorizationStatus == .notDetermined {
            await requestPermission(context: captured)
        } else if let deviceToken, permissionGranted, uploads.isEmpty { uploadToken(deviceToken, renew: true) }
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
        Task { @MainActor in self.registrationErrorMessage = String(localized: "APNs registration failed. Retry notification setup.") }
    }

    private func uploadToken(_ token: String, renew: Bool = false) {
        guard !isSaving, acceptingRegistrations, permissionGranted, relay.supported, let apiClient, let captured = context,
              let setup = confirmed, setup.preferences.enabled, apiClient.isCurrent(captured) else { return }
        // An unchanged callback need not renew a still-valid registration.
        if !renew && token == uploadedToken && setup.registered { return }
        guard !uploads.values.contains(where: { $0.generation == captured.generation }) else { return }
        let id = UUID()
        beginWrite(id)
        // A lost Relay response can hide a committed rotation. Treat the attempt
        // as unconfirmed until Server accepts the new grant or reconciliation
        // actually inspects the still-current grant.
        confirmed?.registered = false
        let upload = Task { @MainActor in
            defer {
                self.uploads[id] = nil
                self.finishWrite(id)
                if self.context?.generation == captured.generation {
                    if let latest = self.deviceToken, latest != token { self.uploadToken(latest) }
                }
            }
            do {
                guard URL(string: captured.serverUrl)?.scheme == "https" else { throw PushSetupError.insecureServer }
                let grant: RelayGrant
                if let pending = self.pendingGrant(captured, url: setup.relayUrl),
                   pending.deviceToken == token, pending.expiresAt > Int64(Date().timeIntervalSince1970) {
                    grant = pending
                } else {
                    grant = try await self.relay.register(token: token, relayUrl: setup.relayUrl, scope: captured.pushScope) {
                        guard apiClient.isCurrent(captured), self.context?.generation == captured.generation else {
                            throw AuthError.staleIdentity
                        }
                    }
                    try self.storage.save(JSONEncoder.snakeCase.encode(PendingPushGrant(grant: grant, url: setup.relayUrl)),
                                          key: self.pendingKey(captured))
                }
                // Keep the original identity's grant available to logout even
                // if the Server response or a subsequent identity check fails.
                self.grants[captured.generation] = (grant, setup.relayUrl)
                guard apiClient.isCurrent(captured), self.context?.generation == captured.generation else { throw AuthError.staleIdentity }
                let content = try self.prepareContentKey(captured)
                let result: PushSetup = try await apiClient.send(
                    "/api/mobile/push/verified-register", method: "POST",
                    body: VerifiedPushRequest(expectedRevision: setup.revision, deviceToken: token, environment: grant.environment,
                                              keyId: grant.keyId, grantId: grant.grantId, grantToken: grant.grantToken,
                                              contentKeyId: content.keyId, contentKey: content.key, deploymentId: content.deploymentId), context: captured
                )
                guard self.ownsWrite(id, captured: captured) else { throw AuthError.staleIdentity }
                guard result.registered, result.revision >= (self.confirmed?.revision ?? 0) else { throw PushSetupError.unavailable }
                self.clearPending(captured)
                self.confirmed = result
                self.uploadedToken = token
                self.registrationErrorMessage = nil
            } catch {
                if self.ownsWrite(id, captured: captured) { self.confirmed?.registered = false }
                if case APIError.httpError(let status, _) = error, status == 403 { self.clearPending(captured) }
                self.report(error, captured: captured)
            }
        }
        uploads[id] = (captured.generation, upload)
    }

    func unregister() async { await unregister(context: context) }

    func unregister(context capturedContext: MobileAuthenticationContext?) async {
        let capturedClient = apiClient
        if context?.generation == capturedContext?.generation {
            acceptingRegistrations = false
            invalidateReads()
        }
        let pending = uploads.values.filter { $0.generation == capturedContext?.generation }.map { $0.task }
        for upload in pending { await upload.value }
        if let capturedClient, let capturedContext {
            do { try await capturedClient.postCleanup("/api/mobile/push/unregister", context: capturedContext) } catch { AppLog.push.error("Push unregister failed; session revocation follows") }
            let binding = grants.removeValue(forKey: capturedContext.generation)
            let pending = storage.load(pendingKey(capturedContext))
                .flatMap { try? JSONDecoder.snakeCase.decode(PendingPushGrant.self, from: $0) }
            clearPending(capturedContext)
            clearContentKey(capturedContext)
            if let grant = binding?.grant ?? pending?.grant, let url = binding?.url ?? pending?.url {
                do { try await relay.revoke(grant, relayUrl: url) } catch {
                    AppLog.push.error("Relay revocation failed; Server session revocation still stops delivery")
                }
            }
        }
        if context?.generation == capturedContext?.generation {
            context = nil
            confirmed = nil
            deviceToken = nil
            uploadedToken = nil
            activeWrite = nil
            permissionRequest = nil
            invalidateReads()
            isSaving = false
        }
    }
}

extension PushNotificationManager {
    func contentKey() -> PushContentKey? {
        storage.load(PushContentKey.storageKey).flatMap { try? JSONDecoder().decode(PushContentKey.self, from: $0) }
    }

    private func prepareContentKey(_ captured: MobileAuthenticationContext) throws -> PushContentKey {
        if let existing = contentKey(), existing.scope == captured.pushScope { return existing }
        let secret = SymmetricKey(size: .bits256).withUnsafeBytes { Data($0) }
        let record = PushContentKey(keyId: UUID().uuidString.lowercased(), key: secret.base64EncodedString(),
                                    deploymentId: captured.serverUrl, userId: captured.userId,
                                    installationId: captured.installationId, scope: captured.pushScope)
        try storage.save(JSONEncoder().encode(record), key: PushContentKey.storageKey)
        return record
    }
    private func clearContentKey(_ captured: MobileAuthenticationContext) {
        if contentKey()?.scope == captured.pushScope { storage.delete(PushContentKey.storageKey) }
        testDelivery.clear(context: captured)
    }
}

private extension PushNotificationManager {
    func requestPermission(context captured: MobileAuthenticationContext) async {
        guard !isSaving, let setup = confirmed, setup.preferences.enabled, acceptingRegistrations,
              let apiClient, context?.generation == captured.generation, apiClient.isCurrent(captured), relay.supported else { return }
        let request = UUID()
        permissionRequest = request
        invalidateReads()
        isSaving = true
        defer { finishPermission(request) }
        do {
            let granted = try await system.requestPermission()
            let status = await system.authorization()
            guard acceptingRegistrations, permissionRequest == request, apiClient.isCurrent(captured),
                  context?.generation == captured.generation, confirmed?.preferences.enabled == true,
                  confirmed?.revision == setup.revision else { return }
            permissionGranted = granted
            authorizationStatus = status
            finishPermission(request)
            if granted { system.register() } else { registrationErrorMessage = String(localized: "Notification permission is disabled. Open system settings to enable it.") }
            if granted, let deviceToken { uploadToken(deviceToken) }
        } catch { report(error, captured: captured) }
    }

    func waitForPendingRegistrations(generation: UUID?) async {
        let tasks = uploads.values.filter { $0.generation == generation }.map { $0.task }
        for task in tasks { await task.value }
    }
    func invalidateReads() {
        latestRead = nil
    }

    func ownsRead(_ read: UUID, captured: MobileAuthenticationContext) -> Bool {
        acceptingRegistrations && !isSaving && latestRead == read
            && context?.generation == captured.generation && apiClient?.isCurrent(captured) == true
    }

    func ownsWrite(_ write: UUID, captured: MobileAuthenticationContext) -> Bool {
        acceptingRegistrations && activeWrite == write && context?.generation == captured.generation
            && apiClient?.isCurrent(captured) == true
    }

    func beginWrite(_ write: UUID) {
        activeWrite = write
        invalidateReads()
        isSaving = true
    }

    func finishWrite(_ write: UUID) {
        guard activeWrite == write else { return }
        activeWrite = nil
        invalidateReads()
        isSaving = permissionRequest != nil
    }

    func finishPermission(_ request: UUID) {
        guard permissionRequest == request else { return }
        permissionRequest = nil
        invalidateReads()
        isSaving = activeWrite != nil
    }

    func pendingKey(_ context: MobileAuthenticationContext) -> String {
        "serverbee_pending_push_" + context.pushScope
    }

    func pendingGrant(_ context: MobileAuthenticationContext, url: String) -> RelayGrant? {
        guard let data = storage.load(pendingKey(context)),
              let pending = try? JSONDecoder.snakeCase.decode(PendingPushGrant.self, from: data), pending.url == url else { return nil }
        return pending.grant
    }

    func clearPending(_ context: MobileAuthenticationContext) { storage.delete(pendingKey(context)) }

    func confirmPreferenceSave(_ setup: PushSetup, captured: MobileAuthenticationContext) {
        // Called only after an owned GET. A registration response cannot prove
        // that a failed preference write saved its intended category choices.
        guard let failed = failedPreferences, failed.generation == captured.generation,
              setup.revision > failed.expectedRevision, setup.preferences == failed.preferences,
              !failed.preferences.security || setup.securityAllowed,
              !(failed.preferences.taskFailure || failed.preferences.taskSuccess) || setup.tasksAllowed == true else { return }
        failedPreferences = nil
    }

    func report(_ error: Error, captured: MobileAuthenticationContext) {
        guard let apiClient, apiClient.isCurrent(captured), context?.generation == captured.generation else { return }
        if case AuthError.staleIdentity = error { return }
        registrationErrorMessage = AccountSecurityViewModel.message(for: error, fallback: String(localized: "Notification setup failed. Retry to confirm registration."))
    }
}

private struct FailedPreferenceSave {
    let generation: UUID
    let expectedRevision: Int64
    let preferences: PushPreferences
    let message: String
}

private struct PendingPushGrant: Codable {
    let grant: RelayGrant
    let url: String
    enum CodingKeys: String, CodingKey { case grant, url }
}

extension PushNotificationManager {
    func configure(apiClient: APIClient) {
        let next = apiClient.captureContext()
        if context?.generation != next?.generation {
            confirmed = nil
            registrationErrorMessage = nil
            failedPreferences = nil
            uploadedToken = nil
            activeWrite = nil
            permissionRequest = nil
            invalidateReads()
            isSaving = false
        }
        self.apiClient = apiClient
        context = next
        acceptingRegistrations = next != nil
        testDelivery.configure(apiClient: apiClient)
        if let record = contentKey(), record.scope != next?.pushScope { storage.delete(PushContentKey.storageKey) }
    }

    func sendTestNotification() async {
        guard !isSaving, acceptingRegistrations else { return }
        await testDelivery.send(setup: confirmed)
    }

    func refreshTestStatus() async { await testDelivery.refresh(setup: confirmed) }
}
