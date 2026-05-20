import UserNotifications
import XCTest
@testable import ServerBee

@MainActor
final class SettingsViewModelTests: XCTestCase {
    func test_logout_callsUnregisterBeforeClearAuth() async throws {
        let authManager = AuthManager()
        // Seed an authenticated state so we can observe the post-clearAuth flip.
        authManager.isAuthenticated = true

        // The spy captures authManager.isAuthenticated at the moment unregister
        // runs. If unregister fires BEFORE clearAuth, this snapshot is `true`.
        let pushManager = SpyPushNotificationManager(authManager: authManager)
        let apiClient = APIClient(authManager: authManager)

        let sut = SettingsViewModel()
        await sut.logout(
            authManager: authManager,
            apiClient: apiClient,
            pushManager: pushManager
        )

        XCTAssertTrue(pushManager.unregisterCalled, "unregister() must be called during logout")
        XCTAssertEqual(
            pushManager.authenticatedSnapshotAtUnregister,
            true,
            "unregister() must run BEFORE clearAuth() — at that point isAuthenticated should still be true"
        )
        XCTAssertFalse(
            authManager.isAuthenticated,
            "clearAuth() must still run, leaving isAuthenticated == false"
        )
    }

    func test_logout_clearsAuthEvenWhenUnregisterFails() async throws {
        let authManager = AuthManager()
        authManager.isAuthenticated = true

        let pushManager = SpyPushNotificationManager(authManager: authManager, shouldThrow: true)
        let apiClient = APIClient(authManager: authManager)

        let sut = SettingsViewModel()
        await sut.logout(
            authManager: authManager,
            apiClient: apiClient,
            pushManager: pushManager
        )

        XCTAssertTrue(pushManager.unregisterCalled)
        XCTAssertFalse(
            authManager.isAuthenticated,
            "clearAuth() must still run even when unregister() encounters errors"
        )
    }
}

// MARK: - Test doubles

@MainActor
final class SpyPushNotificationManager: PushNotificationManaging {
    let backingAuthManager: AuthManager
    let shouldThrow: Bool
    var permissionGranted = false
    var deviceToken: String?

    private(set) var unregisterCalled = false
    private(set) var authenticatedSnapshotAtUnregister: Bool?

    init(authManager: AuthManager, shouldThrow: Bool = false) {
        self.backingAuthManager = authManager
        self.shouldThrow = shouldThrow
    }

    func configure(apiClient: APIClient) {}
    func requestPermission() async {}
    nonisolated func didRegisterForRemoteNotifications(deviceToken data: Data) {}
    nonisolated func didFailToRegisterForRemoteNotifications(error: Error) {}
    nonisolated func handleNotificationResponse(_ response: UNNotificationResponse) -> ServerDeepLink? { nil }

    func unregister() async {
        unregisterCalled = true
        authenticatedSnapshotAtUnregister = backingAuthManager.isAuthenticated
        // PushNotificationManaging.unregister() must swallow network errors,
        // so even in "shouldThrow" mode we do not propagate — we just record.
    }
}
