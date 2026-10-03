import Foundation
import UserNotifications

#if SERVERBEE_EXTENSION_TESTS
// Compile the actual extension entry point in the existing app-hosted unit
// bundle. Production shares these same models through NotificationShared.
@testable import ServerBee
#endif

final class NotificationService: UNNotificationServiceExtension {
    var loadKey: () -> PushContentKey? = {
        SharedPushKeychain.load().flatMap { try? JSONDecoder().decode(PushContentKey.self, from: $0) }
    }
    private var completion: ((UNNotificationContent) -> Void)?
    private var fallback: UNMutableNotificationContent?

    override func didReceive(_ request: UNNotificationRequest, withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void) {
        completion = contentHandler
        fallback = PushNotificationRenderer.render(request.content, key: nil)
        let key = loadKey()
        finish(PushNotificationRenderer.render(request.content, key: key))
    }

    override func serviceExtensionTimeWillExpire() {
        if let fallback { finish(fallback) }
    }

    private func finish(_ content: UNNotificationContent) {
        guard let completion else { return }
        self.completion = nil
        completion(content)
    }
}
