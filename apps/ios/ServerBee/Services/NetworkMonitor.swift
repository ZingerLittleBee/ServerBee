import Network
import SwiftUI

/// Monitors device network connectivity using `NWPathMonitor`.
/// Publishes changes to `isConnected` on the main thread so SwiftUI
/// views can react immediately.
@Observable
final class NetworkMonitor: @unchecked Sendable {
    /// `true` when the device has a usable network path.
    private(set) var isConnected = true

    private let monitor = NWPathMonitor()
    private let queue = DispatchQueue(label: "com.serverbee.networkmonitor")

    /// Begin monitoring. Call once at app launch.
    func start() {
        monitor.pathUpdateHandler = { [weak self] path in
            DispatchQueue.main.async {
                self?.isConnected = path.status == .satisfied
            }
        }
        monitor.start(queue: queue)
    }

    /// Stop monitoring. Safe to call multiple times.
    func stop() {
        monitor.cancel()
    }
}
