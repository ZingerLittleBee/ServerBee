import SwiftUI

/// Holds security events pushed live over the WebSocket so any open Security
/// section can merge them ahead of its REST-loaded history. Bounded so a noisy
/// server can't grow memory without limit.
@MainActor
@Observable
final class SecurityFeedStore {
    private(set) var events: [SecurityEvent] = []

    private let cap = 200

    /// Ingest a live broadcast, newest-first, de-duplicated by id.
    func ingest(_ broadcast: SecurityEventBroadcast) {
        let event = SecurityEvent(broadcast: broadcast)
        guard !events.contains(where: { $0.id == event.id }) else { return }
        events.insert(event, at: 0)
        if events.count > cap {
            events.removeLast(events.count - cap)
        }
    }

    /// Live events for one server, newest-first.
    func events(forServer serverId: String) -> [SecurityEvent] {
        events.filter { $0.serverId == serverId }
    }
}
