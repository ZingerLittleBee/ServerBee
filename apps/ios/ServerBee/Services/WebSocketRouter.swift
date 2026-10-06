import Foundation

/// Fans out incoming `BrowserMessage` frames to the relevant view models.
/// Lives on the main actor because the handlers mutate `@Observable` state.
@MainActor
struct WebSocketRouter {
    let servers: (BrowserMessage) -> Void
    let alerts: (BrowserMessage) -> Void
    var security: (SecurityEventBroadcast) -> Void = { _ in }
    var upgrades: (BrowserMessage) -> Void = { _ in }
    var catalogRefresh: ([String]?) async -> Void = { _ in }

    /// Reconnects may miss catalog events, so a full sync also reloads REST data.
    func dispatchAndRefresh(_ message: BrowserMessage) async {
        dispatch(message)
        switch message {
        case .fullSync:
            await catalogRefresh(nil)
        case .serverCatalogChanged(let serverIds):
            await catalogRefresh(serverIds)
        default:
            break
        }
    }

    func dispatch(_ message: BrowserMessage) {
        switch message {
        case .fullSync:
            // Full sync carries both the server metrics and the upgrade snapshot.
            servers(message)
            upgrades(message)
        case .update, .serverOnline, .serverOffline, .agentAuthorityChanged,
             .capabilitiesChanged, .agentInfoUpdated, .serverCatalogChanged:
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
