import Foundation
import Observation

/// Holds the most recent deep-link request triggered by a push tap.
///
/// `ContentView` observes `pendingDeepLink` and, on a non-nil value, updates
/// its `NavigationStack` path then clears the link by setting it back to nil.
@MainActor
@Observable
final class PushNotificationRouter {
    /// The next deep link to consume. ContentView is responsible for clearing
    /// it once it has updated navigation state.
    var pendingDeepLink: ServerDeepLink?
    private(set) var pendingEnvelope: PushEnvelope?

    func enqueue(envelope: PushEnvelope) { pendingEnvelope = envelope }

    func consumeAccountTarget(context: MobileAuthenticationContext, key: PushContentKey?) -> ServerDeepLink? {
        guard let envelope = pendingEnvelope else { return nil }
        pendingEnvelope = nil
        guard let key, key.scope == context.pushScope, key.deploymentId == context.serverUrl,
              key.userId == context.userId, key.installationId == context.installationId,
              let content = try? PushEnvelopeDecoder.decrypt(envelope, key: key) else { return nil }
        if let alert = content.alert { return .alertDetail(alertKey: alert.alertKey) }
        return .account
    }

    func enqueue(_ link: ServerDeepLink) {
        self.pendingDeepLink = link
    }

    func consume() -> ServerDeepLink? {
        let link = pendingDeepLink
        pendingDeepLink = nil
        return link
    }
}
