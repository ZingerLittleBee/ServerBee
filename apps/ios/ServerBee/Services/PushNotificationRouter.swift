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

    func enqueue(_ link: ServerDeepLink) {
        self.pendingDeepLink = link
    }

    func consume() -> ServerDeepLink? {
        let link = pendingDeepLink
        pendingDeepLink = nil
        return link
    }
}
