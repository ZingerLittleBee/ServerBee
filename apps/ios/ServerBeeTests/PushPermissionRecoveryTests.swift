import Foundation
import XCTest
@testable import ServerBee

extension PushPreferenceRecoveryTests {
    func testDeniedPermissionRecoveryPreservesDraftAndRejectsOldAccountCompletion() async throws {
        let http = PreferenceRecoveryHTTP(security: true)
        PreferenceRecoveryURLProtocol.fixture = http
        let auth = AuthManager()
        login(auth)
        let system = TestPushSystem()
        system.status = .denied
        let manager = manager(auth, system: system)
        await manager.reconcile()
        let confirmed = try XCTUnwrap(manager.confirmed?.preferences)
        var draft = confirmed
        draft.alerts = false
        draft.taskSuccess = true
        http.rejectSave()
        await manager.savePreferences(draft)
        let saveError = try XCTUnwrap(manager.errorMessage)
        XCTAssertEqual(manager.unconfirmedPreferences, draft)
        system.status = .authorized
        await manager.reconcile()
        XCTAssertTrue(manager.permissionGranted)
        XCTAssertEqual(system.permissionRequests, 0)
        let admitted = expectation(description: "permission recovery registers current login")
        http.observeRegistration(admitted)
        manager.didRegisterForRemoteNotifications(deviceToken: Data(repeating: 10, count: 32))
        await fulfillment(of: [admitted], timeout: 3)
        await manager.waitForPendingRegistrations()
        XCTAssertEqual(manager.confirmed?.registered, true)
        XCTAssertEqual(manager.confirmed?.preferences, confirmed)
        XCTAssertEqual(manager.unconfirmedPreferences, draft)
        XCTAssertEqual(manager.errorMessage, saveError)
        XCTAssertEqual(http.registrationAttempts().count, 1)
        let held = expectation(description: "old login registration recovery read suspended")
        http.holdNextRead(held)
        let recovery = Task { await manager.retry() }
        await fulfillment(of: [held], timeout: 3)
        auth.clearAuth()
        login(auth, user: "bob")
        manager.configure(apiClient: APIClient(authManager: auth))
        http.release()
        await recovery.value
        XCTAssertNil(manager.confirmed)
        XCTAssertNil(manager.unconfirmedPreferences)
        XCTAssertNil(manager.contentKey())
        XCTAssertEqual(auth.user?.id, "bob")
        XCTAssertEqual(system.permissionRequests, 0)
        XCTAssertEqual(http.savedPreferences(), [draft])
        XCTAssertEqual(http.registrationAttempts().count, 1, "old read completion must not reach Server registration")
        XCTAssertEqual(http.registrationAttempts().first?.url?.host, "alice.test")
    }
}
