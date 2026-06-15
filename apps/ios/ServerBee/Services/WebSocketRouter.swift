import Foundation

/// Fans out incoming `BrowserMessage` frames to the relevant view models.
/// Lives on the main actor because the handlers mutate `@Observable` state.
@MainActor
struct WebSocketRouter {
    let servers: (BrowserMessage) -> Void
    let alerts: (BrowserMessage) -> Void
    var security: (SecurityEventBroadcast) -> Void = { _ in }
    var upgrades: (BrowserMessage) -> Void = { _ in }

    func dispatch(_ message: BrowserMessage) {
        switch message {
        case .fullSync:
            // Full sync carries both the server metrics and the upgrade snapshot.
            servers(message)
            upgrades(message)
        case .update, .serverOnline, .serverOffline,
             .capabilitiesChanged, .agentInfoUpdated:
            servers(message)
        case .alertEvent:
            alerts(message)
        case .securityEvent(let broadcast):
            security(broadcast)
        case .upgradeProgress, .upgradeResult:
            upgrades(message)
        case .unknown:
            break
        }
    }
}
