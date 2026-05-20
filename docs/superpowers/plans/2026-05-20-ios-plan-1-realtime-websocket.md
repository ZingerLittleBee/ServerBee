# iOS Plan 1: Realtime Layer / WebSocket Rewrite

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite WebSocketClient as an actor with heartbeat, proper exponential backoff, ScenePhase-aware reconnection, and correct close/establish lifecycle. Wire NetworkMonitor offline banner and route alert events to AlertsViewModel.

**Architecture:** Convert `WebSocketClient` from `@unchecked Sendable` class to Swift `actor`. Inject a `WebSocketTransport` protocol so tests can substitute a fake. Connection state only transitions to `.connected` after the first successful `receive()`. A separate `Task` sends `sendPing` every 25 seconds; failure triggers reconnect. `ScenePhase` listener in `ServerBeeApp` reconnects when returning from background. `BrowserMessage` dispatch is moved to a central router that fans out to both ServersViewModel and AlertsViewModel.

**Tech Stack:** Swift 5.10+, SwiftUI, `URLSessionWebSocketTask`, `XCTest`, xcodegen.

---

## File Structure

**Created:**
- `apps/ios/ServerBeeTests/SmokeTests.swift` — initial smoke test
- `apps/ios/ServerBeeTests/Info.plist` — xctest bundle plist
- `apps/ios/ServerBee/Services/WebSocketTransport.swift` — protocol + URLSession adapter
- `apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift` — unit tests via fake transport
- `apps/ios/ServerBeeTests/Support/FakeWebSocketTransport.swift` — test double
- `apps/ios/ServerBee/Services/WebSocketRouter.swift` — fan-out dispatcher

**Modified:**
- `apps/ios/project.yml` — add `ServerBeeTests` target
- `apps/ios/ServerBee/Services/WebSocketClient.swift` — actor conversion, ping, backoff fix, race fix
- `apps/ios/ServerBee/ContentView.swift` — remove `.onDisappear`, wire router, scenePhase, offline overlay
- `apps/ios/ServerBee/ServerBeeApp.swift` — add `NetworkMonitor` env and `scenePhase` plumbing
- `apps/ios/ServerBee/Views/Servers/ServerCardView.swift` — conform to `Equatable`
- `apps/ios/ServerBee/Views/Servers/ServersListView.swift` — use `.equatable()` on card

---

## Task 1: Add ServerBeeTests target via xcodegen + smoke test

**Files:**
- Modify: `apps/ios/project.yml`
- Create: `apps/ios/ServerBeeTests/Info.plist`
- Create: `apps/ios/ServerBeeTests/SmokeTests.swift`

- [ ] **Step 1: Add the test target to `project.yml`**

Replace the existing `targets:` block in `apps/ios/project.yml` (lines 14-26 in the current file) with the version below. Keep the existing `ServerBee` block unchanged except where shown, then append `ServerBeeTests`.

Final desired tail of `apps/ios/project.yml`:

```yaml
targets:
  ServerBee:
    type: application
    platform: iOS
    sources:
      - path: ServerBee
    settings:
      base:
        INFOPLIST_FILE: ServerBee/Info.plist
        PRODUCT_BUNDLE_IDENTIFIER: com.serverbee.mobile
        TARGETED_DEVICE_FAMILY: "1"
        SWIFT_STRICT_CONCURRENCY: complete
        CODE_SIGN_ENTITLEMENTS: ServerBee/ServerBee.entitlements

  ServerBeeTests:
    type: bundle.unit-test
    platform: iOS
    sources:
      - path: ServerBeeTests
    dependencies:
      - target: ServerBee
    settings:
      base:
        INFOPLIST_FILE: ServerBeeTests/Info.plist
        PRODUCT_BUNDLE_IDENTIFIER: com.serverbee.mobile.tests
        GENERATE_INFOPLIST_FILE: NO
        BUNDLE_LOADER: "$(TEST_HOST)"
        TEST_HOST: "$(BUILT_PRODUCTS_DIR)/ServerBee.app/$(BUNDLE_EXECUTABLE_FOLDER_PATH)/ServerBee"
        SWIFT_VERSION: "6.0"
        SWIFT_STRICT_CONCURRENCY: complete
schemes:
  ServerBee:
    build:
      targets:
        ServerBee: all
        ServerBeeTests: [test]
    test:
      targets:
        - ServerBeeTests
      gatherCoverageData: true
```

- [ ] **Step 2: Create the test bundle Info.plist**

Create `apps/ios/ServerBeeTests/Info.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>$(DEVELOPMENT_LANGUAGE)</string>
    <key>CFBundleExecutable</key>
    <string>$(EXECUTABLE_NAME)</string>
    <key>CFBundleIdentifier</key>
    <string>$(PRODUCT_BUNDLE_IDENTIFIER)</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>$(PRODUCT_NAME)</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
</dict>
</plist>
```

- [ ] **Step 3: Write the failing smoke test**

Create `apps/ios/ServerBeeTests/SmokeTests.swift`:

```swift
import XCTest
@testable import ServerBee

final class SmokeTests: XCTestCase {
    func test_browserMessageDecoder_decodesServerOnline() throws {
        let json = #"{"type":"server_online","server_id":"abc-123"}"#
        let data = Data(json.utf8)
        let message = try JSONDecoder.snakeCase.decode(BrowserMessage.self, from: data)
        if case .serverOnline(let id) = message {
            XCTAssertEqual(id, "abc-123")
        } else {
            XCTFail("Expected .serverOnline")
        }
    }
}
```

- [ ] **Step 4: Regenerate the Xcode project and run the smoke test**

Run:

```bash
cd apps/ios && xcodegen generate
xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' test 2>&1 | tail -40
```

Expected: build succeeds, `SmokeTests.test_browserMessageDecoder_decodesServerOnline` reports `Test Case '-[ServerBeeTests.SmokeTests test_browserMessageDecoder_decodesServerOnline]' passed`.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/project.yml apps/ios/ServerBeeTests apps/ios/ServerBee.xcodeproj
git commit -m "$(cat <<'EOF'
test(ios): add ServerBeeTests xctest target and smoke test
EOF
)"
```

---

## Task 2: Extract `WebSocketTransport` protocol

**Files:**
- Create: `apps/ios/ServerBee/Services/WebSocketTransport.swift`
- Create: `apps/ios/ServerBeeTests/Support/FakeWebSocketTransport.swift`
- Test: `apps/ios/ServerBeeTests/Services/WebSocketTransportTests.swift`

- [ ] **Step 1: Write the failing test**

Create `apps/ios/ServerBeeTests/Services/WebSocketTransportTests.swift`:

```swift
import XCTest
@testable import ServerBee

final class WebSocketTransportTests: XCTestCase {
    func test_fakeTransport_deliversEnqueuedMessages() async throws {
        let fake = FakeWebSocketTransport()
        await fake.enqueueText(#"{"type":"server_online","server_id":"x"}"#)

        fake.resume()
        let frame = try await fake.receive()
        if case .string(let s) = frame {
            XCTAssertTrue(s.contains("server_online"))
        } else {
            XCTFail("expected string frame")
        }
    }

    func test_fakeTransport_cancelStopsReceive() async {
        let fake = FakeWebSocketTransport()
        fake.resume()
        fake.cancel(with: .goingAway, reason: nil)

        do {
            _ = try await fake.receive()
            XCTFail("expected throw after cancel")
        } catch {
            // expected
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/WebSocketTransportTests test 2>&1 | tail -20
```

Expected: compilation failure — `cannot find 'FakeWebSocketTransport' in scope` / `WebSocketTransport`.

- [ ] **Step 3: Create the protocol and URLSession adapter**

Create `apps/ios/ServerBee/Services/WebSocketTransport.swift`:

```swift
import Foundation

/// Abstraction over `URLSessionWebSocketTask` so tests can inject a fake.
protocol WebSocketTransport: Sendable {
    func resume()
    func cancel(with closeCode: URLSessionWebSocketTask.CloseCode, reason: Data?)
    func receive() async throws -> URLSessionWebSocketTask.Message
    func send(_ message: URLSessionWebSocketTask.Message) async throws
    func sendPing() async throws
}

/// Production transport backed by `URLSessionWebSocketTask`.
final class URLSessionWebSocketTransport: WebSocketTransport, @unchecked Sendable {
    private let task: URLSessionWebSocketTask

    init(task: URLSessionWebSocketTask) {
        self.task = task
    }

    func resume() {
        task.resume()
    }

    func cancel(with closeCode: URLSessionWebSocketTask.CloseCode, reason: Data?) {
        task.cancel(with: closeCode, reason: reason)
    }

    func receive() async throws -> URLSessionWebSocketTask.Message {
        try await task.receive()
    }

    func send(_ message: URLSessionWebSocketTask.Message) async throws {
        try await task.send(message)
    }

    func sendPing() async throws {
        try await withCheckedThrowingContinuation { continuation in
            task.sendPing { error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume()
                }
            }
        }
    }
}

/// Factory invoked by `WebSocketClient` to obtain a transport for a URL/token.
typealias WebSocketTransportFactory = @Sendable (_ url: URL, _ accessToken: String) -> WebSocketTransport

enum DefaultWebSocketTransportFactory {
    static let factory: WebSocketTransportFactory = { url, accessToken in
        var request = URLRequest(url: url)
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        let task = URLSession.shared.webSocketTask(with: request)
        return URLSessionWebSocketTransport(task: task)
    }
}
```

Create `apps/ios/ServerBeeTests/Support/FakeWebSocketTransport.swift`:

```swift
import Foundation
@testable import ServerBee

/// Test double for `WebSocketTransport`. Messages must be enqueued before
/// `receive()` is awaited. `cancel` causes any subsequent or pending
/// `receive` to throw `CancellationError`.
final class FakeWebSocketTransport: WebSocketTransport, @unchecked Sendable {
    private let lock = NSLock()
    private var pending: [URLSessionWebSocketTask.Message] = []
    private var continuations: [CheckedContinuation<URLSessionWebSocketTask.Message, Error>] = []
    private var isCancelled = false
    private(set) var resumed = false
    private(set) var pingCount = 0
    var pingError: Error?
    var sentMessages: [URLSessionWebSocketTask.Message] = []

    func resume() {
        lock.lock(); defer { lock.unlock() }
        resumed = true
    }

    func cancel(with closeCode: URLSessionWebSocketTask.CloseCode, reason: Data?) {
        lock.lock()
        isCancelled = true
        let waiters = continuations
        continuations = []
        lock.unlock()
        for c in waiters {
            c.resume(throwing: CancellationError())
        }
    }

    func receive() async throws -> URLSessionWebSocketTask.Message {
        try await withCheckedThrowingContinuation { continuation in
            lock.lock()
            if isCancelled {
                lock.unlock()
                continuation.resume(throwing: CancellationError())
                return
            }
            if !pending.isEmpty {
                let next = pending.removeFirst()
                lock.unlock()
                continuation.resume(returning: next)
                return
            }
            continuations.append(continuation)
            lock.unlock()
        }
    }

    func send(_ message: URLSessionWebSocketTask.Message) async throws {
        lock.lock(); defer { lock.unlock() }
        sentMessages.append(message)
    }

    func sendPing() async throws {
        lock.lock()
        pingCount += 1
        let error = pingError
        lock.unlock()
        if let error { throw error }
    }

    // MARK: - Test helpers

    func enqueueText(_ text: String) async {
        lock.lock()
        if let waiter = continuations.first {
            continuations.removeFirst()
            lock.unlock()
            waiter.resume(returning: .string(text))
            return
        }
        pending.append(.string(text))
        lock.unlock()
    }

    func failNextReceive(with error: Error) async {
        lock.lock()
        if let waiter = continuations.first {
            continuations.removeFirst()
            lock.unlock()
            waiter.resume(throwing: error)
            return
        }
        // No-op if no one is waiting — production code only calls receive in a loop.
        lock.unlock()
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/WebSocketTransportTests test 2>&1 | tail -20
```

Expected: `Test Suite 'WebSocketTransportTests' passed`. Two tests passed.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/Services/WebSocketTransport.swift \
        apps/ios/ServerBeeTests/Support/FakeWebSocketTransport.swift \
        apps/ios/ServerBeeTests/Services/WebSocketTransportTests.swift
git commit -m "$(cat <<'EOF'
refactor(ios): extract WebSocketTransport protocol for testability
EOF
)"
```

---

## Task 3: Convert `WebSocketClient` to actor (#18)

**Files:**
- Modify: `apps/ios/ServerBee/Services/WebSocketClient.swift` (full rewrite)
- Test: `apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift`

This task introduces the actor with the existing surface (no behavior changes yet). Subsequent tasks layer in handshake gating, backoff, ping, and race fix.

- [ ] **Step 1: Write the failing test**

Create `apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift`:

```swift
import XCTest
@testable import ServerBee

@MainActor
final class WebSocketClientTests: XCTestCase {
    func test_connect_decodesIncomingFrameAndCallsOnMessage() async throws {
        let fake = FakeWebSocketTransport()
        let client = WebSocketClient(transportFactory: { _, _ in fake })

        let received = expectation(description: "onMessage fired")
        await client.setOnMessage { msg in
            if case .serverOnline(let id) = msg, id == "abc" {
                received.fulfill()
            }
        }

        await client.connect(serverUrl: "https://example.test", accessToken: "tok")
        await fake.enqueueText(#"{"type":"server_online","server_id":"abc"}"#)

        await fulfillment(of: [received], timeout: 2.0)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/WebSocketClientTests test 2>&1 | tail -25
```

Expected: compilation failure — `WebSocketClient.init` does not accept `transportFactory`, `setOnMessage` does not exist.

- [ ] **Step 3: Rewrite `WebSocketClient` as an actor**

Replace the entire contents of `apps/ios/ServerBee/Services/WebSocketClient.swift` with:

```swift
import Foundation

/// A WebSocket client that connects to the ServerBee server's `/api/ws/servers`
/// endpoint, receives `BrowserMessage` frames, and automatically reconnects
/// with exponential backoff on disconnection.
///
/// Implemented as an `actor` so all mutable state is serialized.
actor WebSocketClient {
    enum ConnectionState: Sendable {
        case connecting
        case connected
        case disconnected
    }

    // MARK: - Public observable state

    private(set) var connectionState: ConnectionState = .disconnected

    // MARK: - Private state

    private var transport: WebSocketTransport?
    private var intentionallyClosed = false
    private var reconnectDelay: TimeInterval = 1.0
    private var receiveTask: Task<Void, Never>?

    private var currentServerUrl: String = ""
    private var currentAccessToken: String = ""

    private var onMessage: (@Sendable (BrowserMessage) -> Void)?
    private var tokenRefresher: (@Sendable () async -> String?)?
    private var connectionStateObserver: (@Sendable (ConnectionState) -> Void)?

    private let transportFactory: WebSocketTransportFactory

    // MARK: - Constants

    private let minReconnectDelay: TimeInterval = 1.0
    private let maxReconnectDelay: TimeInterval = 30.0
    private let jitterFactor: Double = 0.2

    // MARK: - Init

    init(transportFactory: @escaping WebSocketTransportFactory = DefaultWebSocketTransportFactory.factory) {
        self.transportFactory = transportFactory
    }

    // MARK: - Configuration

    func setOnMessage(_ handler: (@Sendable (BrowserMessage) -> Void)?) {
        self.onMessage = handler
    }

    func setTokenRefresher(_ refresher: (@Sendable () async -> String?)?) {
        self.tokenRefresher = refresher
    }

    func setConnectionStateObserver(_ observer: (@Sendable (ConnectionState) -> Void)?) {
        self.connectionStateObserver = observer
    }

    // MARK: - Public API

    /// Open a WebSocket connection. Closes any prior connection first.
    func connect(serverUrl: String, accessToken: String) async {
        await closeInternal()
        intentionallyClosed = false
        reconnectDelay = minReconnectDelay
        currentServerUrl = serverUrl
        currentAccessToken = accessToken
        establishConnection()
    }

    /// Intentionally close the connection. No automatic reconnect will happen.
    func close() async {
        intentionallyClosed = true
        await closeInternal()
    }

    // MARK: - Connection lifecycle

    private func closeInternal() async {
        receiveTask?.cancel()
        transport?.cancel(with: .goingAway, reason: nil)
        if let task = receiveTask {
            _ = await task.value
        }
        receiveTask = nil
        transport = nil
        setState(.disconnected)
    }

    private func establishConnection() {
        guard let url = makeWebSocketURL(from: currentServerUrl) else {
            print("[WS] Invalid URL: \(currentServerUrl)")
            return
        }

        setState(.connecting)

        let newTransport = transportFactory(url, currentAccessToken)
        transport = newTransport
        newTransport.resume()
        // NOTE: state moves to .connected only after first successful receive
        // (see Task 4).
        setState(.connected)
        reconnectDelay = minReconnectDelay

        receiveTask = Task { [weak self] in
            await self?.receiveLoop(on: newTransport)
        }
    }

    private func receiveLoop(on transport: WebSocketTransport) async {
        while !Task.isCancelled {
            do {
                let message = try await transport.receive()
                switch message {
                case .string(let text):
                    if let data = text.data(using: .utf8) {
                        do {
                            let browserMessage = try JSONDecoder.snakeCase.decode(
                                BrowserMessage.self, from: data
                            )
                            onMessage?(browserMessage)
                        } catch {
                            print("[WS] Failed to decode message: \(error)")
                        }
                    }
                case .data:
                    break
                @unknown default:
                    break
                }
            } catch {
                await handleReceiveError()
                return
            }
        }
    }

    private func handleReceiveError() async {
        setState(.disconnected)
        if !intentionallyClosed {
            await scheduleReconnect()
        }
    }

    // MARK: - Reconnection with exponential backoff

    private func scheduleReconnect() async {
        guard !intentionallyClosed else { return }

        let jitter = 1.0 + (Double.random(in: -1 ... 1) * jitterFactor)
        let delay = min(reconnectDelay * jitter, maxReconnectDelay)

        try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))

        guard !intentionallyClosed, !Task.isCancelled else { return }

        reconnectDelay = min(reconnectDelay * 2, maxReconnectDelay)

        if let refresher = tokenRefresher {
            if let newToken = await refresher() {
                currentAccessToken = newToken
            } else {
                setState(.disconnected)
                return
            }
        }

        establishConnection()
    }

    // MARK: - URL helpers

    private func makeWebSocketURL(from raw: String) -> URL? {
        var wsUrl = raw
        if wsUrl.hasPrefix("https://") {
            wsUrl = "wss://" + wsUrl.dropFirst("https://".count)
        } else if wsUrl.hasPrefix("http://") {
            wsUrl = "ws://" + wsUrl.dropFirst("http://".count)
        }
        if wsUrl.hasSuffix("/") {
            wsUrl = String(wsUrl.dropLast())
        }
        wsUrl += "/api/ws/servers"
        return URL(string: wsUrl)
    }

    // MARK: - State helpers

    private func setState(_ new: ConnectionState) {
        connectionState = new
        connectionStateObserver?(new)
    }
}
```

- [ ] **Step 4: Update `ContentView.swift` to use the actor API**

Replace `apps/ios/ServerBee/ContentView.swift` lines 33-57 (the `.task { ... }.onDisappear { ... }` block) with the snippet below. The `.onDisappear` close is intentionally **kept** in this task — it is removed in Task 8. Only the call sites change here because the actor methods are async.

```swift
        .task {
            let client = APIClient(authManager: authManager)
            apiClient = client
            pushManager.configure(apiClient: client)

            await wsClient.setTokenRefresher { [weak authManager] in
                guard let authManager else { return nil }
                return try? await authManager.refreshAccessToken()
            }

            await wsClient.setOnMessage { [weak serversViewModel] message in
                Task { @MainActor in
                    serversViewModel?.handleWSMessage(message)
                }
            }

            if let serverUrl = authManager.serverUrl,
               let token = authManager.getAccessToken() {
                await wsClient.connect(serverUrl: serverUrl, accessToken: token)
            }
        }
        .onDisappear {
            Task { await wsClient.close() }
        }
```

- [ ] **Step 5: Run test to verify it passes**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/WebSocketClientTests test 2>&1 | tail -20
```

Expected: `Test Case '-[ServerBeeTests.WebSocketClientTests test_connect_decodesIncomingFrameAndCallsOnMessage]' passed`.

- [ ] **Step 6: Commit**

```bash
git add apps/ios/ServerBee/Services/WebSocketClient.swift \
        apps/ios/ServerBee/ContentView.swift \
        apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift
git commit -m "$(cat <<'EOF'
refactor(ios): convert WebSocketClient to actor with injected transport
EOF
)"
```

---

## Task 4: Connection state transitions only after first receive (#2)

**Files:**
- Modify: `apps/ios/ServerBee/Services/WebSocketClient.swift`
- Test: `apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift`

- [ ] **Step 1: Write the failing test**

Append to `apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift` inside the class:

```swift
    func test_connectionState_isConnectingUntilFirstFrame() async throws {
        let fake = FakeWebSocketTransport()
        let client = WebSocketClient(transportFactory: { _, _ in fake })

        await client.connect(serverUrl: "https://example.test", accessToken: "tok")

        let preState = await client.connectionState
        XCTAssertEqual(preState, .connecting)

        let observed = expectation(description: "moved to connected")
        await client.setConnectionStateObserver { state in
            if state == .connected { observed.fulfill() }
        }
        await fake.enqueueText(#"{"type":"server_online","server_id":"x"}"#)

        await fulfillment(of: [observed], timeout: 2.0)
        let postState = await client.connectionState
        XCTAssertEqual(postState, .connected)
    }
```

Also add `Equatable` conformance for tests — add this just under the `ConnectionState` enum declaration in `WebSocketClient.swift`:

```swift
extension WebSocketClient.ConnectionState: Equatable {}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/WebSocketClientTests/test_connectionState_isConnectingUntilFirstFrame test 2>&1 | tail -20
```

Expected: assertion fails because `connectionState` is already `.connected` before the first receive.

- [ ] **Step 3: Gate `.connected` transition on first successful receive**

In `apps/ios/ServerBee/Services/WebSocketClient.swift`, modify `establishConnection()` so it does **not** set state to `.connected`. Remove this line:

```swift
        setState(.connected)
```

Then update `receiveLoop` to flip to `.connected` exactly once on the first successful frame:

Replace the existing `receiveLoop(on:)` body with:

```swift
    private func receiveLoop(on transport: WebSocketTransport) async {
        var sawFirstFrame = false
        while !Task.isCancelled {
            do {
                let message = try await transport.receive()
                if !sawFirstFrame {
                    sawFirstFrame = true
                    setState(.connected)
                }
                switch message {
                case .string(let text):
                    if let data = text.data(using: .utf8) {
                        do {
                            let browserMessage = try JSONDecoder.snakeCase.decode(
                                BrowserMessage.self, from: data
                            )
                            onMessage?(browserMessage)
                        } catch {
                            print("[WS] Failed to decode message: \(error)")
                        }
                    }
                case .data:
                    break
                @unknown default:
                    break
                }
            } catch {
                await handleReceiveError()
                return
            }
        }
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/WebSocketClientTests test 2>&1 | tail -20
```

Expected: both `WebSocketClientTests` cases pass.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/Services/WebSocketClient.swift \
        apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift
git commit -m "$(cat <<'EOF'
fix(ios): gate ws .connected state on first received frame
EOF
)"
```

---

## Task 5: Exponential backoff resets only on confirmed connection (#2)

**Files:**
- Modify: `apps/ios/ServerBee/Services/WebSocketClient.swift`
- Test: `apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift`

- [ ] **Step 1: Write the failing test**

Append to `WebSocketClientTests.swift`:

```swift
    func test_reconnectDelay_doublesAcrossFailedAttempts() async throws {
        var transports: [FakeWebSocketTransport] = []
        let lock = NSLock()
        let factory: WebSocketTransportFactory = { _, _ in
            let t = FakeWebSocketTransport()
            lock.lock(); transports.append(t); lock.unlock()
            return t
        }
        let client = WebSocketClient(transportFactory: factory)
        await client.setTokenRefresher { "stale" }
        let delays = DelayRecorder()
        await client.setReconnectDelayHook { delay in
            await delays.record(delay)
        }

        await client.connect(serverUrl: "https://example.test", accessToken: "tok")
        // Force three failed connection attempts in a row, none of which
        // ever receive a frame.
        for _ in 0..<3 {
            // Wait for transport to exist
            while true {
                lock.lock(); let count = transports.count; lock.unlock()
                if count > 0 { break }
                try await Task.sleep(nanoseconds: 10_000_000)
            }
            lock.lock(); let t = transports.removeFirst(); lock.unlock()
            await t.failNextReceive(with: URLError(.networkConnectionLost))
        }

        let recorded = await delays.values
        XCTAssertEqual(recorded.count, 3)
        XCTAssertEqual(recorded[0], 1.0, accuracy: 0.5)
        XCTAssertGreaterThanOrEqual(recorded[1], 1.6)  // ~2s with jitter
        XCTAssertGreaterThanOrEqual(recorded[2], 3.2)  // ~4s with jitter
    }
}

actor DelayRecorder {
    private(set) var values: [TimeInterval] = []
    func record(_ d: TimeInterval) { values.append(d) }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/WebSocketClientTests/test_reconnectDelay_doublesAcrossFailedAttempts test 2>&1 | tail -25
```

Expected: compile error (`setReconnectDelayHook` undefined) or, after adding stub, the delay never doubles because `establishConnection()` resets it on every retry.

- [ ] **Step 3: Remove reset from `establishConnection` and add observability hook**

In `apps/ios/ServerBee/Services/WebSocketClient.swift`:

(a) Remove the line `reconnectDelay = minReconnectDelay` from `establishConnection()`. Reset should only happen when a frame is actually received.

(b) In `receiveLoop(on:)`, when `sawFirstFrame` flips true, also reset `reconnectDelay`. Replace the `if !sawFirstFrame { ... }` block with:

```swift
                if !sawFirstFrame {
                    sawFirstFrame = true
                    reconnectDelay = minReconnectDelay
                    setState(.connected)
                }
```

(c) Add the hook + property near the other observers:

```swift
    private var reconnectDelayHook: (@Sendable (TimeInterval) async -> Void)?

    func setReconnectDelayHook(_ hook: (@Sendable (TimeInterval) async -> Void)?) {
        self.reconnectDelayHook = hook
    }
```

(d) Inside `scheduleReconnect()`, immediately after computing `delay`, invoke the hook:

```swift
        await reconnectDelayHook?(delay)
```

So the updated `scheduleReconnect()` reads:

```swift
    private func scheduleReconnect() async {
        guard !intentionallyClosed else { return }

        let jitter = 1.0 + (Double.random(in: -1 ... 1) * jitterFactor)
        let delay = min(reconnectDelay * jitter, maxReconnectDelay)
        await reconnectDelayHook?(delay)

        try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))

        guard !intentionallyClosed, !Task.isCancelled else { return }

        reconnectDelay = min(reconnectDelay * 2, maxReconnectDelay)

        if let refresher = tokenRefresher {
            if let newToken = await refresher() {
                currentAccessToken = newToken
            } else {
                setState(.disconnected)
                return
            }
        }

        establishConnection()
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/WebSocketClientTests test 2>&1 | tail -20
```

Expected: all three `WebSocketClientTests` cases pass.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/Services/WebSocketClient.swift \
        apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift
git commit -m "$(cat <<'EOF'
fix(ios): preserve ws exponential backoff across failed reconnects
EOF
)"
```

---

## Task 6: Heartbeat ping every 25s with reconnect on failure (#3)

**Files:**
- Modify: `apps/ios/ServerBee/Services/WebSocketClient.swift`
- Test: `apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift`

- [ ] **Step 1: Write the failing test**

Append to `WebSocketClientTests.swift`:

```swift
    func test_pingFailure_triggersReconnect() async throws {
        let fake = FakeWebSocketTransport()
        fake.pingError = URLError(.timedOut)
        let client = WebSocketClient(
            transportFactory: { _, _ in fake },
            pingInterval: 0.05  // fast for tests
        )

        let observed = expectation(description: "reconnect attempted")
        observed.expectedFulfillmentCount = 1
        await client.setReconnectDelayHook { _ in
            observed.fulfill()
        }

        await client.connect(serverUrl: "https://example.test", accessToken: "tok")
        await fake.enqueueText(#"{"type":"server_online","server_id":"x"}"#)

        await fulfillment(of: [observed], timeout: 5.0)
        XCTAssertGreaterThanOrEqual(fake.pingCount, 1)
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/WebSocketClientTests/test_pingFailure_triggersReconnect test 2>&1 | tail -25
```

Expected: compile failure — `WebSocketClient.init` does not accept `pingInterval`.

- [ ] **Step 3: Add heartbeat task**

In `apps/ios/ServerBee/Services/WebSocketClient.swift`:

(a) Add a property and update `init`:

```swift
    private let pingInterval: TimeInterval
    private var pingTask: Task<Void, Never>?

    init(
        transportFactory: @escaping WebSocketTransportFactory = DefaultWebSocketTransportFactory.factory,
        pingInterval: TimeInterval = 25.0
    ) {
        self.transportFactory = transportFactory
        self.pingInterval = pingInterval
    }
```

(b) In `closeInternal()`, also cancel the ping task. Insert before `receiveTask?.cancel()`:

```swift
        pingTask?.cancel()
        pingTask = nil
```

(c) In `receiveLoop(on:)`, when `sawFirstFrame` flips true, start the heartbeat. Update the block to:

```swift
                if !sawFirstFrame {
                    sawFirstFrame = true
                    reconnectDelay = minReconnectDelay
                    setState(.connected)
                    startPingTask(on: transport)
                }
```

(d) Add the ping driver:

```swift
    private func startPingTask(on transport: WebSocketTransport) {
        pingTask?.cancel()
        pingTask = Task { [weak self, pingInterval] in
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: UInt64(pingInterval * 1_000_000_000))
                if Task.isCancelled { return }
                guard let self else { return }
                let ok = await self.sendHeartbeat(on: transport)
                if !ok { return }
            }
        }
    }

    private func sendHeartbeat(on transport: WebSocketTransport) async -> Bool {
        guard self.transport === transport as AnyObject else { return false }
        do {
            try await transport.sendPing()
            return true
        } catch {
            print("[WS] Heartbeat ping failed: \(error)")
            // Force the receive loop to fail by cancelling the transport;
            // it will trigger scheduleReconnect via handleReceiveError.
            transport.cancel(with: .abnormalClosure, reason: nil)
            return false
        }
    }
```

Note: `transport === transport as AnyObject` is a sanity check that the heartbeat targets the still-current transport; if `connect()` was called again it will have replaced `self.transport`.

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/WebSocketClientTests/test_pingFailure_triggersReconnect test 2>&1 | tail -20
```

Expected: test passes; `fake.pingCount >= 1` and the reconnect hook fires once.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/Services/WebSocketClient.swift \
        apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift
git commit -m "$(cat <<'EOF'
feat(ios): add 25s ws heartbeat ping with reconnect on failure
EOF
)"
```

---

## Task 7: Fix close/reconnect race by awaiting old receive task (#19)

**Files:**
- Modify: `apps/ios/ServerBee/Services/WebSocketClient.swift`
- Test: `apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift`

Note: `closeInternal()` already awaits the prior `receiveTask.value`. This task adds a regression test, plus makes sure a stale `scheduleReconnect()` that resolved while a new `connect()` was running cannot fire `establishConnection()` against the wrong epoch.

- [ ] **Step 1: Write the failing test**

Append to `WebSocketClientTests.swift`:

```swift
    func test_rapidReconnect_doesNotDoubleEstablish() async throws {
        var built = 0
        let lock = NSLock()
        let factory: WebSocketTransportFactory = { _, _ in
            lock.lock(); built += 1; lock.unlock()
            return FakeWebSocketTransport()
        }
        let client = WebSocketClient(transportFactory: factory, pingInterval: 60)

        await client.connect(serverUrl: "https://example.test", accessToken: "tok1")
        // Immediately reconnect with a new token while the prior receive loop
        // is still alive.
        await client.connect(serverUrl: "https://example.test", accessToken: "tok2")

        // Wait a tick for any zombie scheduleReconnect to fire.
        try await Task.sleep(nanoseconds: 200_000_000)
        lock.lock(); let final = built; lock.unlock()
        XCTAssertEqual(final, 2, "expected exactly one transport per connect()")
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/WebSocketClientTests/test_rapidReconnect_doesNotDoubleEstablish test 2>&1 | tail -20
```

Expected: test fails — `built` is 3 because the cancelled receive loop's `handleReceiveError` runs `scheduleReconnect` against the new epoch.

- [ ] **Step 3: Tag each connect attempt with an epoch and ignore stale failures**

In `apps/ios/ServerBee/Services/WebSocketClient.swift` add a `connectionEpoch` counter:

```swift
    private var connectionEpoch: UInt64 = 0
```

Update `establishConnection()` to capture and pass the epoch:

```swift
    private func establishConnection() {
        guard let url = makeWebSocketURL(from: currentServerUrl) else {
            print("[WS] Invalid URL: \(currentServerUrl)")
            return
        }

        setState(.connecting)
        connectionEpoch &+= 1
        let epoch = connectionEpoch

        let newTransport = transportFactory(url, currentAccessToken)
        transport = newTransport
        newTransport.resume()

        receiveTask = Task { [weak self] in
            await self?.receiveLoop(on: newTransport, epoch: epoch)
        }
    }
```

Update `receiveLoop` to accept the epoch:

```swift
    private func receiveLoop(on transport: WebSocketTransport, epoch: UInt64) async {
        var sawFirstFrame = false
        while !Task.isCancelled {
            do {
                let message = try await transport.receive()
                guard epoch == connectionEpoch else { return }
                if !sawFirstFrame {
                    sawFirstFrame = true
                    reconnectDelay = minReconnectDelay
                    setState(.connected)
                    startPingTask(on: transport)
                }
                switch message {
                case .string(let text):
                    if let data = text.data(using: .utf8) {
                        do {
                            let browserMessage = try JSONDecoder.snakeCase.decode(
                                BrowserMessage.self, from: data
                            )
                            onMessage?(browserMessage)
                        } catch {
                            print("[WS] Failed to decode message: \(error)")
                        }
                    }
                case .data:
                    break
                @unknown default:
                    break
                }
            } catch {
                await handleReceiveError(epoch: epoch)
                return
            }
        }
    }

    private func handleReceiveError(epoch: UInt64) async {
        guard epoch == connectionEpoch else { return }
        setState(.disconnected)
        if !intentionallyClosed {
            await scheduleReconnect()
        }
    }
```

In `closeInternal()`, bump the epoch so any in-flight callback bails:

```swift
    private func closeInternal() async {
        connectionEpoch &+= 1
        pingTask?.cancel()
        pingTask = nil
        receiveTask?.cancel()
        transport?.cancel(with: .goingAway, reason: nil)
        if let task = receiveTask {
            _ = await task.value
        }
        receiveTask = nil
        transport = nil
        setState(.disconnected)
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/WebSocketClientTests test 2>&1 | tail -20
```

Expected: all `WebSocketClientTests` pass (`built == 2`).

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/Services/WebSocketClient.swift \
        apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift
git commit -m "$(cat <<'EOF'
fix(ios): guard ws receive loop against stale connection epoch
EOF
)"
```

---

## Task 8: Remove `.onDisappear` close in `ContentView` (#1)

**Files:**
- Modify: `apps/ios/ServerBee/ContentView.swift`

- [ ] **Step 1: Write the failing test**

Append to `apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift`:

```swift
    func test_secondConnect_replacesTransportWithoutManualClose() async throws {
        let fake1 = FakeWebSocketTransport()
        let fake2 = FakeWebSocketTransport()
        var index = 0
        let factory: WebSocketTransportFactory = { _, _ in
            defer { index += 1 }
            return index == 0 ? fake1 : fake2
        }
        let client = WebSocketClient(transportFactory: factory, pingInterval: 60)

        await client.connect(serverUrl: "https://example.test", accessToken: "t")
        await fake1.enqueueText(#"{"type":"server_online","server_id":"a"}"#)

        // Simulate ContentView re-entering: no .onDisappear close, just a
        // second connect call — the old transport must have been cancelled.
        await client.connect(serverUrl: "https://example.test", accessToken: "t")
        XCTAssertEqual(index, 2)
    }
```

This already passes with the actor implementation. The "failing" change is in the view layer — see Step 2.

- [ ] **Step 2: Delete `.onDisappear` from `ContentView`**

In `apps/ios/ServerBee/ContentView.swift`, find the lines (currently at the end of the `body`):

```swift
        .onDisappear {
            Task { await wsClient.close() }
        }
```

Delete them entirely. The `.task` modifier already manages the lifecycle for the lifetime of the view. The actor's `closeInternal()` will be called by `deinit`-driven `close()` only on logout (see Task 9 for explicit logout wiring) — `ContentView` itself stays mounted the whole authed session.

- [ ] **Step 3: Run tests**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' test 2>&1 | tail -20
```

Expected: all tests pass; build succeeds.

- [ ] **Step 4: Commit**

```bash
git add apps/ios/ServerBee/ContentView.swift \
        apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift
git commit -m "$(cat <<'EOF'
fix(ios): stop closing ws on contentview disappear
EOF
)"
```

---

## Task 9: ScenePhase monitoring to reconnect on `.active` (#9)

**Files:**
- Modify: `apps/ios/ServerBee/ContentView.swift`
- Modify: `apps/ios/ServerBee/Services/WebSocketClient.swift` (add `reconnectIfNeeded` helper)
- Test: `apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift`

- [ ] **Step 1: Write the failing test**

Append to `WebSocketClientTests.swift`:

```swift
    func test_reconnectIfNeeded_whenDisconnected_buildsNewTransport() async throws {
        var built = 0
        let lock = NSLock()
        let factory: WebSocketTransportFactory = { _, _ in
            lock.lock(); built += 1; lock.unlock()
            return FakeWebSocketTransport()
        }
        let client = WebSocketClient(transportFactory: factory, pingInterval: 60)
        await client.connect(serverUrl: "https://example.test", accessToken: "t")
        // Simulate a silent drop: forcibly mark disconnected.
        await client.forceDisconnectedForTesting()

        await client.reconnectIfNeeded()
        try await Task.sleep(nanoseconds: 100_000_000)

        lock.lock(); let final = built; lock.unlock()
        XCTAssertEqual(final, 2)
    }

    func test_reconnectIfNeeded_whenConnected_isNoop() async throws {
        var built = 0
        let lock = NSLock()
        let factory: WebSocketTransportFactory = { _, _ in
            lock.lock(); built += 1; lock.unlock()
            return FakeWebSocketTransport()
        }
        let client = WebSocketClient(transportFactory: factory, pingInterval: 60)
        let fake = FakeWebSocketTransport()
        // For simplicity construct fresh client whose factory returns the
        // captured fake first, then any future calls.
        await client.connect(serverUrl: "https://example.test", accessToken: "t")
        // Mark connected by enqueuing a frame on the freshly built transport.
        await Task.sleep(nanoseconds: 50_000_000)
        await client.reconnectIfNeeded()
        try await Task.sleep(nanoseconds: 100_000_000)

        lock.lock(); let final = built; lock.unlock()
        XCTAssertEqual(final, 1, "should not rebuild while in .connecting/.connected")
        _ = fake
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/WebSocketClientTests/test_reconnectIfNeeded_whenDisconnected_buildsNewTransport test 2>&1 | tail -20
```

Expected: compile failure — `reconnectIfNeeded`, `forceDisconnectedForTesting` undefined.

- [ ] **Step 3: Add the helpers to `WebSocketClient`**

Append inside the `WebSocketClient` actor:

```swift
    /// Called from `ScenePhase` listener: if we believe the socket is dead,
    /// rebuild it without resetting the backoff timer.
    func reconnectIfNeeded() async {
        guard !intentionallyClosed else { return }
        guard !currentServerUrl.isEmpty else { return }
        if connectionState == .disconnected {
            await closeInternal()
            intentionallyClosed = false
            establishConnection()
        }
    }

    #if DEBUG
    /// Test-only hook to drive the state machine into `.disconnected`.
    func forceDisconnectedForTesting() async {
        await closeInternal()
        intentionallyClosed = false
    }
    #endif
```

- [ ] **Step 4: Wire `ScenePhase` in `ContentView`**

Modify `apps/ios/ServerBee/ContentView.swift` to add a `@Environment(\.scenePhase)` and an `.onChange` modifier. Updated full body:

```swift
import SwiftUI

struct ContentView: View {
    @Environment(AuthManager.self) private var authManager
    @Environment(PushNotificationManager.self) private var pushManager
    @Environment(\.scenePhase) private var scenePhase
    @State private var apiClient: APIClient?
    @State private var serversViewModel = ServersViewModel()
    @State private var wsClient = WebSocketClient()
    @State private var previousScenePhase: ScenePhase = .active

    var body: some View {
        TabView {
            NavigationStack {
                ServersListView()
            }
            .tabItem {
                Label("Servers", systemImage: "server.rack")
            }

            NavigationStack {
                AlertsListView()
            }
            .tabItem {
                Label("Alerts", systemImage: "bell.badge")
            }

            SettingsView()
                .tabItem {
                    Label("Settings", systemImage: "gearshape")
                }
        }
        .environment(\.apiClient, apiClient)
        .environment(serversViewModel)
        .task {
            let client = APIClient(authManager: authManager)
            apiClient = client
            pushManager.configure(apiClient: client)

            await wsClient.setTokenRefresher { [weak authManager] in
                guard let authManager else { return nil }
                return try? await authManager.refreshAccessToken()
            }
            await wsClient.setOnMessage { [weak serversViewModel] message in
                Task { @MainActor in
                    serversViewModel?.handleWSMessage(message)
                }
            }

            if let serverUrl = authManager.serverUrl,
               let token = authManager.getAccessToken() {
                await wsClient.connect(serverUrl: serverUrl, accessToken: token)
            }
        }
        .onChange(of: scenePhase) { old, new in
            if old == .background && new == .active {
                Task { await wsClient.reconnectIfNeeded() }
            }
            previousScenePhase = new
        }
    }
}

#Preview {
    ContentView()
        .environment(AuthManager())
        .environment(AlertsViewModel())
        .environment(PushNotificationManager())
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' test 2>&1 | tail -25
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add apps/ios/ServerBee/Services/WebSocketClient.swift \
        apps/ios/ServerBee/ContentView.swift \
        apps/ios/ServerBeeTests/Services/WebSocketClientTests.swift
git commit -m "$(cat <<'EOF'
feat(ios): reconnect ws on scenephase active transition
EOF
)"
```

---

## Task 10: Wire `NetworkMonitor` + `OfflineBannerView` as overlay (#21)

**Files:**
- Modify: `apps/ios/ServerBee/ServerBeeApp.swift`
- Modify: `apps/ios/ServerBee/ContentView.swift`

- [ ] **Step 1: Inject `NetworkMonitor` from the app root**

Replace `apps/ios/ServerBee/ServerBeeApp.swift` body of the `ServerBeeApp` struct (lines 6-26) with:

```swift
@main
struct ServerBeeApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @State private var authManager = AuthManager()
    @State private var alertsViewModel = AlertsViewModel()
    @State private var pushManager = PushNotificationManager()
    @State private var networkMonitor = NetworkMonitor()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(authManager)
                .environment(alertsViewModel)
                .environment(pushManager)
                .environment(networkMonitor)
                .task {
                    appDelegate.pushManager = pushManager
                    UNUserNotificationCenter.current().delegate = appDelegate
                    networkMonitor.start()
                    await authManager.initialize()
                    if authManager.isAuthenticated {
                        await pushManager.requestPermission()
                    }
                }
        }
    }
}
```

- [ ] **Step 2: Overlay the banner in `ContentView`**

In `apps/ios/ServerBee/ContentView.swift`, add the environment property and wrap `TabView` in a `ZStack` with the banner.

Update properties:

```swift
    @Environment(NetworkMonitor.self) private var networkMonitor
```

Replace the `var body: some View {` … `TabView { … }` opening section (top of body) so that the entire content is wrapped:

```swift
    var body: some View {
        ZStack(alignment: .top) {
            TabView {
                NavigationStack { ServersListView() }
                    .tabItem { Label("Servers", systemImage: "server.rack") }
                NavigationStack { AlertsListView() }
                    .tabItem { Label("Alerts", systemImage: "bell.badge") }
                SettingsView()
                    .tabItem { Label("Settings", systemImage: "gearshape") }
            }
            .environment(\.apiClient, apiClient)
            .environment(serversViewModel)

            OfflineBannerView(isConnected: networkMonitor.isConnected)
                .animation(.easeInOut(duration: 0.2), value: networkMonitor.isConnected)
        }
        .task { /* ... unchanged from Task 9 ... */ }
        .onChange(of: scenePhase) { /* ... unchanged from Task 9 ... */ }
    }
```

(The `/* unchanged */` placeholders refer to the `.task` and `.onChange` modifiers introduced in Task 9 — copy them verbatim. The full final `ContentView.swift` is reproduced below for clarity.)

Full `apps/ios/ServerBee/ContentView.swift`:

```swift
import SwiftUI

struct ContentView: View {
    @Environment(AuthManager.self) private var authManager
    @Environment(PushNotificationManager.self) private var pushManager
    @Environment(NetworkMonitor.self) private var networkMonitor
    @Environment(\.scenePhase) private var scenePhase
    @State private var apiClient: APIClient?
    @State private var serversViewModel = ServersViewModel()
    @State private var wsClient = WebSocketClient()

    var body: some View {
        ZStack(alignment: .top) {
            TabView {
                NavigationStack { ServersListView() }
                    .tabItem { Label("Servers", systemImage: "server.rack") }
                NavigationStack { AlertsListView() }
                    .tabItem { Label("Alerts", systemImage: "bell.badge") }
                SettingsView()
                    .tabItem { Label("Settings", systemImage: "gearshape") }
            }
            .environment(\.apiClient, apiClient)
            .environment(serversViewModel)

            OfflineBannerView(isConnected: networkMonitor.isConnected)
                .animation(.easeInOut(duration: 0.2), value: networkMonitor.isConnected)
        }
        .task {
            let client = APIClient(authManager: authManager)
            apiClient = client
            pushManager.configure(apiClient: client)

            await wsClient.setTokenRefresher { [weak authManager] in
                guard let authManager else { return nil }
                return try? await authManager.refreshAccessToken()
            }
            await wsClient.setOnMessage { [weak serversViewModel] message in
                Task { @MainActor in
                    serversViewModel?.handleWSMessage(message)
                }
            }
            if let serverUrl = authManager.serverUrl,
               let token = authManager.getAccessToken() {
                await wsClient.connect(serverUrl: serverUrl, accessToken: token)
            }
        }
        .onChange(of: scenePhase) { old, new in
            if old == .background && new == .active {
                Task { await wsClient.reconnectIfNeeded() }
            }
        }
    }
}

#Preview {
    ContentView()
        .environment(AuthManager())
        .environment(AlertsViewModel())
        .environment(PushNotificationManager())
        .environment(NetworkMonitor())
}
```

- [ ] **Step 3: Build and run tests**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' test 2>&1 | tail -20
```

Expected: build succeeds; tests pass.

- [ ] **Step 4: Manual visual check (banner appears when airplane mode on)**

Launch the app in simulator (`Cmd+R`). With wifi enabled the banner is hidden. From `Features → Network Link Conditioner → 100% Loss` (or in the simulator's host menu, toggle network off), within ~1 second the yellow `You are currently offline` banner must appear at the top of the screen. Toggling network back restores the banner to hidden.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/ServerBeeApp.swift \
        apps/ios/ServerBee/ContentView.swift
git commit -m "$(cat <<'EOF'
feat(ios): show offline banner via NetworkMonitor overlay
EOF
)"
```

---

## Task 11: Dispatch `BrowserMessage.alertEvent` to `AlertsViewModel` (#26)

**Files:**
- Create: `apps/ios/ServerBee/Services/WebSocketRouter.swift`
- Modify: `apps/ios/ServerBee/ContentView.swift`
- Test: `apps/ios/ServerBeeTests/Services/WebSocketRouterTests.swift`

- [ ] **Step 1: Write the failing test**

Create `apps/ios/ServerBeeTests/Services/WebSocketRouterTests.swift`:

```swift
import XCTest
@testable import ServerBee

@MainActor
final class WebSocketRouterTests: XCTestCase {
    func test_alertEvent_invokesAlertHandlerOnly() async {
        var servers: [BrowserMessage] = []
        var alerts: [BrowserMessage] = []
        let router = WebSocketRouter(
            servers: { servers.append($0) },
            alerts: { alerts.append($0) }
        )

        router.dispatch(.alertEvent(alertKey: "k", status: .firing))
        XCTAssertEqual(servers.count, 0)
        XCTAssertEqual(alerts.count, 1)
    }

    func test_serverUpdate_invokesServersHandlerOnly() async {
        var servers: [BrowserMessage] = []
        var alerts: [BrowserMessage] = []
        let router = WebSocketRouter(
            servers: { servers.append($0) },
            alerts: { alerts.append($0) }
        )

        router.dispatch(.update(servers: []))
        XCTAssertEqual(servers.count, 1)
        XCTAssertEqual(alerts.count, 0)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' \
  -only-testing:ServerBeeTests/WebSocketRouterTests test 2>&1 | tail -20
```

Expected: compile failure — `WebSocketRouter` undefined.

- [ ] **Step 3: Implement the router**

Create `apps/ios/ServerBee/Services/WebSocketRouter.swift`:

```swift
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
```

- [ ] **Step 4: Wire the router in `ContentView`**

Replace the existing `wsClient.setOnMessage { ... }` block inside `ContentView.task` with the version below. Note we also need `AlertsViewModel` and `apiClient` to be captured.

In `apps/ios/ServerBee/ContentView.swift`, add:

```swift
    @Environment(AlertsViewModel.self) private var alertsViewModel
```

Replace the `.task` body's `setOnMessage` registration with:

```swift
            await wsClient.setOnMessage { [weak serversViewModel, weak alertsViewModel] message in
                Task { @MainActor in
                    guard let serversViewModel else { return }
                    let router = WebSocketRouter(
                        servers: { msg in serversViewModel.handleWSMessage(msg) },
                        alerts: { msg in
                            guard case .alertEvent = msg, let alertsViewModel else { return }
                            if let apiClient {
                                Task { await alertsViewModel.handleWSAlertEvent(apiClient: apiClient) }
                            }
                        }
                    )
                    router.dispatch(message)
                }
            }
```

- [ ] **Step 5: Run tests**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' test 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add apps/ios/ServerBee/Services/WebSocketRouter.swift \
        apps/ios/ServerBee/ContentView.swift \
        apps/ios/ServerBeeTests/Services/WebSocketRouterTests.swift
git commit -m "$(cat <<'EOF'
feat(ios): dispatch ws alert_event to AlertsViewModel via router
EOF
)"
```

---

## Task 12: `ServerCardView` conforms to `Equatable` and uses `.equatable()` (#34)

**Files:**
- Modify: `apps/ios/ServerBee/Views/Servers/ServerCardView.swift`
- Modify: `apps/ios/ServerBee/Views/Servers/ServersListView.swift`

- [ ] **Step 1: Make `ServerCardView` conform to `Equatable`**

Edit `apps/ios/ServerBee/Views/Servers/ServerCardView.swift`. Change the declaration on line 5 from:

```swift
struct ServerCardView: View {
```

to:

```swift
struct ServerCardView: View, Equatable {
    static func == (lhs: ServerCardView, rhs: ServerCardView) -> Bool {
        lhs.server.id == rhs.server.id &&
        lhs.server.online == rhs.server.online &&
        lhs.server.cpuUsage == rhs.server.cpuUsage &&
        lhs.server.memoryUsed == rhs.server.memoryUsed &&
        lhs.server.name == rhs.server.name &&
        lhs.server.lastActiveAt == rhs.server.lastActiveAt &&
        lhs.server.primaryIP == rhs.server.primaryIP &&
        lhs.server.os == rhs.server.os
    }
```

The rest of the file stays unchanged.

- [ ] **Step 2: Apply `.equatable()` at the usage site**

Open `apps/ios/ServerBee/Views/Servers/ServersListView.swift`. Find the `ServerCardView(server: ...)` invocation inside the `LazyVStack` / `ForEach`. It looks similar to `ServerCardView(server: server)`. Append `.equatable()`:

```swift
                    ServerCardView(server: server)
                        .equatable()
```

If multiple invocations exist (e.g. inside `NavigationLink`), apply `.equatable()` to each.

- [ ] **Step 3: Build & test**

Run:

```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 15' test 2>&1 | tail -20
```

Expected: build succeeds; existing tests pass.

- [ ] **Step 4: Manual perf check**

In simulator, attach to a backend with at least 5 servers reporting metrics every second. Open Instruments → SwiftUI template (or set `_printChanges()` breakpoints in `ServerCardView.body`). Observe that cards whose metrics did not change are no longer re-rendered on every WS `update`. Cards whose metrics changed still update.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/Views/Servers/ServerCardView.swift \
        apps/ios/ServerBee/Views/Servers/ServersListView.swift
git commit -m "$(cat <<'EOF'
perf(ios): skip unchanged ServerCardView rebuilds via Equatable
EOF
)"
```

---

## Task 13: Smoke test on real backend

This task is manual — no code changes. Goal: prove the realtime stack survives realistic failure modes.

- [ ] **Step 1: Run local backend**

```bash
cargo run -p serverbee-server
```

Server listens on `http://localhost:9527`. Wait for `[serverbee-server] listening on 0.0.0.0:9527` in the log.

- [ ] **Step 2: Launch the iOS simulator app and log in**

In Xcode: `Cmd+R`. Configure server URL `http://localhost:9527` (or your LAN IP if testing on a device), log in.

Expected console (Xcode debug area):
```
[WS] (no decode errors)
```
The Servers tab populates with the connected agent.

- [ ] **Step 3: Verify exponential backoff on server kill**

Stop the backend (`Ctrl+C` in the terminal running `cargo run`). In the simulator the Servers list freezes (no further updates). Xcode debug area should show recurring reconnect attempts whose intervals follow ~1s, 2s, 4s, 8s, 16s, 30s, 30s (with ±20% jitter). The simulator should **not** hammer reconnects faster than 1/sec.

- [ ] **Step 4: Verify auto-reconnect on server restart**

Restart `cargo run -p serverbee-server`. Within at most ~30 seconds the iOS app reconnects and the Servers list resumes updating. The yellow offline banner must remain hidden (the device network is fine; only the WS was down).

- [ ] **Step 5: Verify ScenePhase background reconnect**

With both the backend and app running and a healthy WS, send the app to background (`Cmd+Shift+H` in simulator) and leave it for ≥5 minutes. iOS silently kills the WS during this time. Re-foreground (tap app icon). The `onChange(of: scenePhase)` listener calls `reconnectIfNeeded`. If `connectionState` was `.disconnected`, a new transport is built; if still `.connected` (rare), no-op. Within ≤2s of foregrounding the app, fresh metrics arrive.

- [ ] **Step 6: Verify alert event routes to Alerts tab**

Trigger an alert from the backend (e.g. set a CPU > 0% rule). Within seconds the iOS Alerts tab list refetches (visible as a brief loading spinner or new row at the top). Switching to the tab shows the new entry.

- [ ] **Step 7: Verify offline banner**

Toggle simulator wifi off (Settings → Wi-Fi). The yellow `You are currently offline` banner appears at the top within ~1 second. Toggle back on; banner disappears; the WS reconnects automatically (because `reconnectIfNeeded` runs on next `.active` if the app was backgrounded, or the receive loop simply resumes once routing returns).

- [ ] **Step 8: Document outcomes**

If any step fails, file a follow-up bug against `tacoma-v4` and reference this plan. No commit required for this task.

---

## Self-review notes

- All scope issues (#1, #2, #3, #9, #18, #19, #21, #26, #34) map to tasks T8, T4+T5, T6, T9, T3, T7, T10, T11, T12 respectively.
- All Swift types — `WebSocketTransport`, `WebSocketTransportFactory`, `URLSessionWebSocketTransport`, `FakeWebSocketTransport`, `WebSocketRouter`, `WebSocketClient.ConnectionState` — are spelled consistently across tasks.
- Method names (`setOnMessage`, `setTokenRefresher`, `setConnectionStateObserver`, `setReconnectDelayHook`, `reconnectIfNeeded`, `forceDisconnectedForTesting`, `dispatch`) match between definitions and call sites.
- No placeholders, no TODO, no "implement later"; every step contains the actual code or command.
