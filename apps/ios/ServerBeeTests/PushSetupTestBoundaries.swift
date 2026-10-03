import Foundation
import UserNotifications
@testable import ServerBee

@MainActor
final class TestPushSystem: PushSystemBoundary {
    var status: UNAuthorizationStatus = .authorized
    var permissionRequests = 0
    var registrations = 0
    var permissionHook: (() async -> Void)?
    func authorization() async -> UNAuthorizationStatus { status }
    func requestPermission() async throws -> Bool {
        permissionRequests += 1
        await permissionHook?()
        return status == .authorized
    }
    func register() { registrations += 1 }
}

enum PushSetupTestData {
    static func response(enabled: Bool = true, registered: Bool = false, revision: Int64 = 1, preferences: PushPreferences? = nil, securityAllowed: Bool = true, tasksAllowed: Bool? = nil) -> Data {
        let taskPermission = tasksAllowed ?? securityAllowed
        let selected = preferences ?? PushPreferences(enabled: enabled, alerts: true, security: false, taskFailure: true, taskSuccess: false)
        return Data("""
        {"data":{"revision":\(revision),"preferences":{"enabled":\(selected.enabled),"alerts":\(selected.alerts),"security":\(selected.security),
        "task_failure":\(selected.taskFailure),"task_success":\(selected.taskSuccess)},
        "tasks_allowed":\(taskPermission),"task_failure_available":true,"security_allowed":\(securityAllowed),"registered":\(registered),
        "test_available":true,"delivery_available":false}}
        """.utf8)
    }
    static func body(_ request: URLRequest) -> [String: Any] {
        var data = request.httpBody ?? Data()
        if data.isEmpty, let stream = request.httpBodyStream {
            stream.open()
            defer { stream.close() }
            var buffer = [UInt8](repeating: 0, count: 1024)
            while stream.hasBytesAvailable {
                let count = stream.read(&buffer, maxLength: buffer.count)
                if count <= 0 { break }
                data.append(contentsOf: buffer.prefix(count))
            }
        }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
    }

}

@MainActor
final class MemoryPushSetupStorage: PushSetupStorage {
    var values: [String: Data] = [:]
    func load(_ key: String) -> Data? { values[key] }
    func save(_ data: Data, key: String) throws { values[key] = data }
    func delete(_ key: String) { values[key] = nil }
}

/// Track only external HTTP requests so teardown cancels any held request and
/// late responses cannot resume an already cancelled URLSession operation.
final class PendingURLProtocolRequests: @unchecked Sendable {
    private let lock = NSLock()
    private var requests: [ObjectIdentifier: URLProtocol] = [:]
    func begin(_ request: URLProtocol) {
        lock.lock(); defer { lock.unlock() }
        requests[ObjectIdentifier(request)] = request
    }
    func finish(_ request: URLProtocol) -> Bool {
        lock.lock(); defer { lock.unlock() }
        return requests.removeValue(forKey: ObjectIdentifier(request)) != nil
    }
    func cancelAll() {
        lock.lock()
        let pending = Array(requests.values)
        requests.removeAll()
        lock.unlock()
        for request in pending { request.client?.urlProtocol(request, didFailWithError: URLError(.cancelled)) }
    }
}
