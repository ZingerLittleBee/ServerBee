import Foundation

/// Owns one installation's test identity across ambiguous replies and restarts.
/// Confirmed admission is never replayed, even if registration changes later.
@MainActor
@Observable
final class PushTestDelivery {
    private(set) var result: TestPushResponse?
    private(set) var isTesting = false
    private(set) var errorMessage: String?

    private let storage: any PushSetupStorage
    private let storageKey = "serverbee_pending_push_test"
    private var apiClient: APIClient?
    private var context: MobileAuthenticationContext?
    private var request: TestPushRequest?
    private var admission = TestPushAdmission.unknown
    private var operation: UUID?

    init(storage: any PushSetupStorage) { self.storage = storage }

    func configure(apiClient: APIClient) {
        let next = apiClient.captureContext()
        if context?.generation != next?.generation {
            operation = nil
            isTesting = false
            result = nil
            errorMessage = nil
            request = nil
        }
        self.apiClient = apiClient
        context = next
        if request == nil, let saved = saved() {
            if saved.scope == next?.pushScope {
                request = saved.request
                admission = saved.admission ?? .unknown
            } else { storage.delete(storageKey) }
        }
    }

    func clear(context captured: MobileAuthenticationContext) {
        if saved()?.scope == captured.pushScope { storage.delete(storageKey) }
        guard context?.generation == captured.generation else { return }
        operation = nil
        isTesting = false
        request = nil
        admission = .unknown
        result = nil
        errorMessage = nil
    }

    func send(setup: PushSetup?) async {
        guard !isTesting, let setup, setup.registered, setup.preferences.enabled,
              let apiClient, let captured = context, apiClient.isCurrent(captured) else { return }
        let owner = begin()
        defer { finish(owner) }
        do {
            if request == nil || result?.isPending == false {
                request = TestPushRequest(eventId: UUID().uuidString.lowercased(), expectedRevision: setup.revision)
                admission = .unadmitted
                result = nil
            } else if try await lookup(setup: setup, captured: captured, owner: owner) { return }
            try validate(captured, owner: owner)
            guard let request else { return }
            // Any failure after sending may hide successful admission. Persist
            // unknown before the network boundary, preserving the same UUID.
            admission = .unknown
            try persist(captured)
            let reply: TestPushResponse = try await apiClient.send(
                "/api/mobile/push/test", method: "POST", body: request, context: captured
            )
            try validate(captured, owner: owner)
            try record(reply, captured: captured)
        } catch { report(error, captured: captured, owner: owner) }
    }

    func refresh(setup: PushSetup?) async {
        guard !isTesting, request != nil, let apiClient, let captured = context,
              apiClient.isCurrent(captured) else { return }
        let owner = begin()
        defer { finish(owner) }
        do { _ = try await lookup(setup: setup, captured: captured, owner: owner) } catch { report(error, captured: captured, owner: owner) }
    }
}

private extension PushTestDelivery {
    /// A 404 can authorize rebinding only when admission was never confirmed.
    /// Retain the UUID: a late original POST and the retry still share one key.
    func lookup(setup: PushSetup?, captured: MobileAuthenticationContext, owner: UUID) async throws -> Bool {
        guard let apiClient, let request else { return false }
        do {
            let reply: TestPushResponse = try await apiClient.get("/api/mobile/push/test/\(request.eventId)", context: captured)
            try validate(captured, owner: owner)
            try record(reply, captured: captured)
            return true
        } catch APIError.httpError(let code, _) where code == 404 {
            try validate(captured, owner: owner)
            // An admitted receipt cannot safely be recreated from a missing
            // status, for example after a Server restores an older database.
            guard admission != .admitted else { throw PushSetupError.unavailable }
            self.request = TestPushRequest(eventId: request.eventId, expectedRevision: setup?.revision ?? request.expectedRevision)
            admission = .unadmitted
            result = nil
            try persist(captured)
            errorMessage = nil
            return false
        }
    }

    func record(_ reply: TestPushResponse, captured: MobileAuthenticationContext) throws {
        result = reply
        admission = .admitted
        try persist(captured)
        errorMessage = nil
    }

    func persist(_ captured: MobileAuthenticationContext) throws {
        guard let request else { return }
        try storage.save(JSONEncoder().encode(SavedTestPush(scope: captured.pushScope, request: request, admission: admission)), key: storageKey)
    }

    func saved() -> SavedTestPush? {
        storage.load(storageKey).flatMap { try? JSONDecoder().decode(SavedTestPush.self, from: $0) }
    }

    func begin() -> UUID {
        let owner = UUID()
        operation = owner
        isTesting = true
        return owner
    }

    func finish(_ owner: UUID) {
        guard operation == owner else { return }
        operation = nil
        isTesting = false
    }

    func validate(_ captured: MobileAuthenticationContext, owner: UUID) throws {
        guard operation == owner, context?.generation == captured.generation,
              apiClient?.isCurrent(captured) == true else { throw AuthError.staleIdentity }
    }

    func report(_ error: Error, captured: MobileAuthenticationContext, owner: UUID) {
        guard operation == owner, context?.generation == captured.generation,
              apiClient?.isCurrent(captured) == true else { return }
        if case AuthError.staleIdentity = error { return }
        errorMessage = AccountSecurityViewModel.message(for: error, fallback: String(localized: "Notification setup failed. Retry to confirm registration."))
    }
}
