import Foundation

/// Fans out incoming `BrowserMessage` frames to the relevant view models.
/// Lives on the main actor because both handlers mutate `@Observable` VMs.
@MainActor
struct WebSocketRouter {
    let servers: (BrowserMessage) -> Void
    let alerts: (BrowserMessage) -> Void

    func dispatch(_ message: BrowserMessage) {
        switch message {
        case .fullSync, .update, .serverOnline, .serverOffline,
             .capabilitiesChanged, .agentInfoUpdated:
            servers(message)
        case .alertEvent:
            alerts(message)
        }
    }
}
