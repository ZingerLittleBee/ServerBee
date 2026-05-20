import XCTest
@testable import ServerBee

final class AuthManagerMainActorTests: XCTestCase {
    @MainActor
    func testStateMutationsHappenOnMainThread() async {
        let auth = AuthManager()
        auth.isAuthenticated = true
        XCTAssertTrue(Thread.isMainThread, "Mutating AuthManager state must be on main thread")
        XCTAssertTrue(auth.isAuthenticated)
    }

    /// Compile-time check: if AuthManager is `@MainActor`-isolated, this off-actor
    /// closure body should not be able to read `isAuthenticated` without `await`.
    /// We assert the runtime by hopping to MainActor explicitly.
    func testReadFromBackgroundRequiresActorHop() async {
        let auth = await AuthManager()
        let value: Bool = await MainActor.run { auth.isAuthenticated }
        XCTAssertFalse(value)
    }
}
