import Foundation

// MARK: - Refresh Coordinator

struct ScopedAccessToken: Sendable {
    let generation: UUID
    let accessToken: String
}

/// Serialises concurrent token-refresh attempts.
///
/// Semantics:
/// - For each login generation at most one `refreshFn` is in flight.
/// - While a refresh is in flight, callers for the same login `await` on the existing
///   task so we don't hammer the refresh endpoint or burn a one-time-use
///   refresh token.
/// - **On success:** every waiter receives the new access token.
/// - **On failure:** the in-flight attempt's error is propagated ONLY to the
///   caller who initiated it. Subsequent waiters are released and each gets a
///   fresh attempt at `refreshFn`. This lets a transient network failure for
///   the first caller not penalise queued callers — the next one retries.
///
/// Internal so tests can drive `refresh(generation:using:)` directly without going
/// through `AuthManager.refreshAccessToken()` — see RefreshCoordinatorTests.
actor RefreshCoordinator {
    private var inFlight: [UUID: (id: UUID, task: Task<ScopedAccessToken, Error>)] = [:]
    // A scheduler boundary permits deterministic tests of a completed task
    // whose owner has not resumed to remove it. Production does not suspend here.
    private let beforeCompletion: (@Sendable (ScopedAccessToken) async -> Void)?

    init(beforeCompletion: (@Sendable (ScopedAccessToken) async -> Void)? = nil) {
        self.beforeCompletion = beforeCompletion
    }

    func refresh(
        generation: UUID,
        using refreshFn: @Sendable @escaping () async throws -> String
    ) async throws -> ScopedAccessToken {
        while let existing = inFlight[generation] {
            do {
                return try await existing.task.value
            } catch {
                if inFlight[generation]?.id == existing.id { inFlight[generation] = nil }
            }
        }

        let id = UUID()
        let task = Task {
            let token = try await refreshFn()
            return ScopedAccessToken(generation: generation, accessToken: token)
        }
        inFlight[generation] = (id, task)
        do {
            let result = try await task.value
            if let beforeCompletion { await beforeCompletion(result) }
            if inFlight[generation]?.id == id { inFlight[generation] = nil }
            return result
        } catch {
            if inFlight[generation]?.id == id { inFlight[generation] = nil }
            throw error
        }
    }
}
