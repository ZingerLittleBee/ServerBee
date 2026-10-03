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
    var testResult: TestPushResponse? { testDelivery.result }
    var isTesting: Bool { testDelivery.isTesting }
    private let testDelivery: PushTestDelivery
    private let system: any PushSystemBoundary
    private let environment: String
    private let storage: any PushSetupStorage
    private var apiClient: APIClient?
    private var context: MobileAuthenticationContext?
    private var acceptingRegistrations = false
    private var uploads: [UUID: (generation: UUID, task: Task<Void, Never>)] = [:]
    private var uploadedToken: String?
    private var registrationErrorMessage: String?
    private var failedPreferences: FailedPreferenceSave?
    // One write/permission operation owns setup at a time. Each read owns a
    // unique token, invalidated by writes and superseded by newer reads even
    // when permissions change without a registration revision change.
    private var latestRead: UUID?
    private var activeWrite: UUID?
    private var permissionRequest: UUID?

    init(
        system: any PushSystemBoundary = NativePushSystem(), storage: any PushSetupStorage = KeychainPushSetupStorage(),
        environment: String? = nil
    ) {
        self.system = system
        self.environment = environment
            ?? Bundle.main.object(forInfoDictionaryKey: "ServerBeeAPNSEnvironment") as? String ?? ""
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
            let setup: PushSetup = try await apiClient.get("/api/mobile/push/settings", context: captured)
            guard ownsRead(read, captured: captured),
                  setup.revision >= (confirmed?.revision ?? 0) else { return false }
            authorizationStatus = status
            permissionGranted = status == .authorized || status == .provisional || status == .ephemeral
            confirmed = setup
            confirmPreferenceSave(setup, captured: captured)
            registrationErrorMessage = nil
            if !setup.preferences.enabled { clearContentKey(captured) }
            if setup.preferences.enabled && permissionGranted {
                system.register()
                if let deviceToken, !setup.registered || deviceToken != uploadedToken {
                    uploadToken(deviceToken)
                }
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
            registrationErrorMessage = nil
            // Permission and local key cleanup follow the confirmed PUT.
            // Release this write so permission completion can start an upload.
            finishWrite(write)
            if setup.preferences.enabled {
                if !confirmed.preferences.enabled {
                    await requestPermission(context: captured)
                } else if permissionGranted, let deviceToken { uploadToken(deviceToken) }
            } else {
                uploadedToken = nil
                clearContentKey(captured)
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
        } else if let deviceToken, permissionGranted, uploads.isEmpty { uploadToken(deviceToken, force: true) }
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

    private func uploadToken(_ token: String, force: Bool = false) {
        guard !isSaving, acceptingRegistrations, permissionGranted, let apiClient, let captured = context,
              let setup = confirmed, setup.preferences.enabled, apiClient.isCurrent(captured) else { return }
        // An unchanged callback need not repeat a confirmed registration.
        if !force && token == uploadedToken && setup.registered { return }
        guard !uploads.values.contains(where: { $0.generation == captured.generation }) else { return }
        let id = UUID()
        beginWrite(id)
        // Show confirmation only after this authenticated Server write succeeds.
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
                try await apiClient.requireDeletionRecovery(context: captured)
                guard apiClient.isCurrent(captured), self.context?.generation == captured.generation else { throw AuthError.staleIdentity }
                guard ["sandbox", "production"].contains(self.environment) else { throw PushSetupError.unavailable }
                let content = try self.prepareContentKey(captured)
                let result: PushSetup = try await apiClient.send(
                    "/api/mobile/push/encrypted-register", method: "POST",
                    body: PushRegistrationRequest(expectedRevision: setup.revision, deviceToken: token, environment: self.environment,
                                              contentKeyId: content.keyId, contentKey: content.key, deploymentId: content.deploymentId), context: captured
                )
                guard self.ownsWrite(id, captured: captured) else { throw AuthError.staleIdentity }
                guard result.registered, result.revision >= (self.confirmed?.revision ?? 0) else { throw PushSetupError.unavailable }
                self.confirmed = result
                self.uploadedToken = token
                self.registrationErrorMessage = nil
            } catch {
                if self.ownsWrite(id, captured: captured) { self.confirmed?.registered = false }
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
            clearContentKey(capturedContext)
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
              let apiClient, context?.generation == captured.generation, apiClient.isCurrent(captured) else { return }
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
