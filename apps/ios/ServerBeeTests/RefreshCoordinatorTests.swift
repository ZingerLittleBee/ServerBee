import XCTest
@testable import ServerBee

@MainActor
final class RefreshCoordinatorTests: XCTestCase {
    override func setUp() async throws {
        URLProtocol.registerClass(URLProtocolStub.self)
        URLProtocolStub.stubResponse = nil
        URLProtocolStub.stubError = nil
        URLProtocolStub.stubResponseFactory = nil
    }

    override func tearDown() async throws {
        URLProtocol.unregisterClass(URLProtocolStub.self)
        URLProtocolStub.stubResponse = nil
        URLProtocolStub.stubError = nil
        URLProtocolStub.stubResponseFactory = nil
    }

    /// Concurrent successful refresh attempts coalesce: every caller observes
    /// the same token without firing the underlying `refreshFn` more than once.
    ///
    /// NOTE: This test exercises the `RefreshCoordinator` actor directly rather
    /// than going through `AuthManager.refreshAccessToken()` because the current
    /// `MobileTokenResponse` model has a `convertFromSnakeCase` + explicit
    /// `CodingKeys` rawValue conflict that prevents the success-path JSON from
    /// decoding (a latent bug Plan 5 fixes by removing the rawValues).
    func testConcurrentCallersCoalesceOnSuccess() async throws {
        let coordinator = RefreshCoordinator()
        actor Counter { var n = 0; func bump() -> Int { n += 1; return n } }
        let counter = Counter()

        let refreshFn: @Sendable () async throws -> String = {
            _ = await counter.bump()
            // Give the other tasks time to enqueue as waiters.
            try? await Task.sleep(nanoseconds: 50_000_000)
            return "shared-token"
        }

        async let t1 = coordinator.refresh(using: refreshFn)
        async let t2 = coordinator.refresh(using: refreshFn)
        async let t3 = coordinator.refresh(using: refreshFn)
        let results = try await [t1, t2, t3]

        XCTAssertEqual(Set(results), ["shared-token"])
        let invocations = await counter.n
        XCTAssertEqual(invocations, 1, "refreshFn should fire exactly once when callers coalesce")
    }

    /// When the leader's `refreshFn` fails transiently, the next waiter retries
    /// (rather than inheriting the leader's stale error).
    func testFirstWaiterRetriesOnTransientFailure() async throws {
        let coordinator = RefreshCoordinator()

        actor Counter { var n = 0; func bump() -> Int { n += 1; return n } }
        let counter = Counter()

        let refreshFn: @Sendable () async throws -> String = {
            let attempt = await counter.bump()
            if attempt == 1 {
                throw AuthError.refreshNetworkFailure(nil)
            }
            return "new"
        }

        // First caller fails transiently.
        do {
            _ = try await coordinator.refresh(using: refreshFn)
            XCTFail("First call should fail")
        } catch let err as AuthError {
            if case .refreshNetworkFailure = err { /* ok */ } else {
                XCTFail("Expected refreshNetworkFailure, got \(err)")
            }
        }

        // Second caller (a new attempt) should succeed.
        let token = try await coordinator.refresh(using: refreshFn)
        XCTAssertEqual(token, "new")
    }

    /// Concurrent waiters: when the in-flight leader fails, second waiter
    /// should re-attempt the `refreshFn` rather than inheriting the leader's
    /// error. The old continuation-based impl would propagate the same error
    /// to every waiter.
    func testConcurrentWaiterRetriesIfLeaderFails() async throws {
        let coordinator = RefreshCoordinator()
        actor Counter { var n = 0; func bump() -> Int { n += 1; return n } }
        let counter = Counter()

        let refreshFn: @Sendable () async throws -> String = {
            let attempt = await counter.bump()
            // Slow down so all callers enqueue before the leader resolves.
            try? await Task.sleep(nanoseconds: 50_000_000)
            if attempt == 1 {
                throw AuthError.refreshNetworkFailure(nil)
            }
            return "second-attempt"
        }

        // Launch leader + waiter concurrently.
        async let leader = coordinator.refresh(using: refreshFn)
        // Give the leader a tick to claim inFlight.
        try? await Task.sleep(nanoseconds: 5_000_000)
        async let waiter = coordinator.refresh(using: refreshFn)

        // Leader fails.
        do {
            _ = try await leader
            XCTFail("Leader should fail")
        } catch {
            // expected
        }

        // Waiter should NOT inherit leader's error; it gets a fresh attempt.
        let token = try await waiter
        XCTAssertEqual(token, "second-attempt")
    }
}
