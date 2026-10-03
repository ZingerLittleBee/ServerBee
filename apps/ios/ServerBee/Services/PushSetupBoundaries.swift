import CryptoKit
import Foundation
import UIKit
import UserNotifications

@MainActor
protocol PushSystemBoundary {
    func authorization() async -> UNAuthorizationStatus
    func requestPermission() async throws -> Bool
    func register()
}

@MainActor
struct NativePushSystem: PushSystemBoundary {
    func authorization() async -> UNAuthorizationStatus {
        // Older SDKs lack Sendable annotations on both settings and callback.
        // Keep the callback nonisolated and send only the extracted status.
        await withCheckedContinuation { continuation in
            UNUserNotificationCenter.current().getNotificationSettings { @Sendable settings in
                continuation.resume(returning: settings.authorizationStatus)
            }
        }
    }
    func requestPermission() async throws -> Bool {
        try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .badge, .sound])
    }
    func register() { UIApplication.shared.registerForRemoteNotifications() }
}

extension MobileAuthenticationContext {
    /// Stable across refresh/restart, distinct for every paired login, including
    /// replacement logins for the same account. Bootstrap keeps the original
    /// scope credential separate from its new deletion-only capability.
    var pushScope: String {
        let identity = [serverUrl, userId, installationId, revocationToken ?? refreshToken ?? generation.uuidString]
        return SHA256.hash(data: Data(identity.joined(separator: "|").utf8))
            .map { String(format: "%02x", $0) }.joined()
    }
}

@MainActor
protocol PushSetupStorage {
    func load(_ key: String) -> Data?
    func save(_ data: Data, key: String) throws
    func delete(_ key: String)
}

@MainActor
struct KeychainPushSetupStorage: PushSetupStorage {
    func load(_ key: String) -> Data? { key == PushContentKey.storageKey ? SharedPushKeychain.load() : KeychainService.load(for: key) }
    func save(_ data: Data, key: String) throws {
        if key == PushContentKey.storageKey { try SharedPushKeychain.save(data) } else { try KeychainService.save(data, for: key) }
    }
    func delete(_ key: String) {
        if key == PushContentKey.storageKey { SharedPushKeychain.delete() } else { KeychainService.delete(for: key) }
    }
}

enum PushSetupError: Error, LocalizedError {
    case unavailable
    case insecureServer
    var errorDescription: String? {
        switch self {
        case .unavailable: String(localized: "Notification setup is unavailable. Monitoring and login still work.")
        case .insecureServer: String(localized: "Encrypted notification setup requires an HTTPS Server.")
        }
    }
}
