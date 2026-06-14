import Foundation

/// Fans out incoming `BrowserMessage` frames to the relevant view models.
/// Lives on the main actor because the handlers mutate `@Observable` state.
@MainActor
struct WebSocketRouter {
    let servers: (BrowserMessage) -> Void
    let alerts: (BrowserMessage) -> Void
    var security: (SecurityEventBroadcast) -> Void = { _ in }

    func dispatch(_ message: BrowserMessage) {
        switch message {
        case .fullSync, .update, .serverOnline, .serverOffline,
             .capabilitiesChanged, .agentInfoUpdated:
            servers(message)
        case .alertEvent:
            alerts(message)
        case .securityEvent(let broadcast):
            security(broadcast)
        case .unknown:
            break
        }
    }
}
