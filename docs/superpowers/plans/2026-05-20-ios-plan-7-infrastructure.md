# iOS Plan 7: Infrastructure and Build Hygiene

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prepare the iOS app for App Store review and long-term maintenance: justify the ATS posture with a runtime warning + review notes, replace ad-hoc `print` with `os.Logger`, expand the test target's coverage to critical pure-logic paths, integrate SwiftLint, and document scoping decisions (iPhone-only).

**Architecture:** A single `AppLog` enum exposes categorized `os.Logger` instances. The login/settings UI surfaces a yellow banner when the configured server uses `http://`. SwiftLint is integrated as a Swift Package plugin so it ships with the project. Test coverage focuses on pure-logic regression surfaces: model decoding, formatters, refresh coordination.

**Tech Stack:** Swift, OSLog, xcodegen, SwiftLint, XCTest.

**Depends on:** Plan 1 (`ServerBeeTests` target). Plan 5 may have already added some tests — coordinate by skipping duplicates.

---

## Task 1: Document ATS decision and add HTTP warning banner component

**Files:**
- Create: `apps/ios/ServerBee/Views/Components/InsecureURLBanner.swift`

- [ ] **Step 1: Create the banner component**

```swift
// apps/ios/ServerBee/Views/Components/InsecureURLBanner.swift
import SwiftUI

/// A yellow warning banner shown when the configured server URL uses `http://`.
/// ATS is disabled globally in Info.plist because users self-host on arbitrary
/// IPs/domains, but we surface this trade-off to the user at runtime so they
/// can opt into HTTPS when possible. See `AppStoreReviewNotes.md` for the
/// App Store review justification.
struct InsecureURLBanner: View {
    let serverUrl: String

    var body: some View {
        if shouldShow {
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.yellow)
                Text(
                    String(
                        localized:
                            "This server uses an unencrypted HTTP connection. Credentials and metrics are sent in clear text. Use HTTPS whenever possible."
                    )
                )
                .font(.footnote)
                .foregroundStyle(.primary)
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.yellow.opacity(0.15), in: RoundedRectangle(cornerRadius: 8))
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(Color.yellow.opacity(0.5), lineWidth: 1)
            )
        }
    }

    private var shouldShow: Bool {
        let trimmed = serverUrl.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return trimmed.hasPrefix("http://")
    }
}

#Preview {
    VStack {
        InsecureURLBanner(serverUrl: "http://192.168.1.10:9527")
        InsecureURLBanner(serverUrl: "https://serverbee.example.com")
    }
    .padding()
}
```

- [ ] **Step 2: Regenerate Xcode project**

Run: `cd apps/ios && xcodegen generate`
Expected: `Generated project successfully`

- [ ] **Step 3: Build to confirm compile**

Run: `cd apps/ios && xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator' -quiet`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 4: Commit**

```bash
git add apps/ios/ServerBee/Views/Components/InsecureURLBanner.swift
git commit -m "feat(ios): add InsecureURLBanner component for http:// server URLs"
```

---

## Task 2: Wire InsecureURLBanner into LoginView

**Files:**
- Modify: `apps/ios/ServerBee/Views/Auth/LoginView.swift`

- [ ] **Step 1: Add the banner under the server URL field**

Locate the `serverUrlInput` `TextField` in `LoginView.swift`. Immediately after the TextField (or the form section containing it), insert:

```swift
InsecureURLBanner(serverUrl: authViewModel.serverUrlInput)
    .padding(.horizontal)
```

If the TextField sits inside a `Form` `Section`, place the banner immediately after that section. The exact insertion point: directly below the line that renders the server URL `TextField`, still inside the same `VStack`/`Form` parent.

- [ ] **Step 2: Build**

Run: `cd apps/ios && xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator' -quiet`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 3: Manual visual check**

Run the app in the iOS Simulator. On the Login screen, type `http://1.2.3.4:9527` into the server URL field — the yellow banner should appear. Clear or change to `https://...` — banner disappears.

- [ ] **Step 4: Commit**

```bash
git add apps/ios/ServerBee/Views/Auth/LoginView.swift
git commit -m "feat(ios): show insecure URL banner in LoginView"
```

---

## Task 3: Wire InsecureURLBanner into SettingsView

**Files:**
- Modify: `apps/ios/ServerBee/Views/Settings/SettingsView.swift`

- [ ] **Step 1: Render banner near the server URL row**

In `SettingsView.swift`, locate the row/Section that displays the current server URL (likely a `LabeledContent` or `Text` showing `authManager.serverUrl`). Add a non-interactive section header banner directly above (or below) that row:

```swift
Section {
    InsecureURLBanner(serverUrl: authManager.serverUrl ?? "")
        .listRowBackground(Color.clear)
        .listRowSeparator(.hidden)
}
```

Place this `Section` immediately before the section that displays the server URL.

- [ ] **Step 2: Build**

Run: `cd apps/ios && xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator' -quiet`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 3: Manual visual check**

Launch Simulator, log in with an `http://` server URL, open Settings. Confirm banner is visible. Log out and log in via `https://` — confirm banner is hidden in Settings.

- [ ] **Step 4: Commit**

```bash
git add apps/ios/ServerBee/Views/Settings/SettingsView.swift
git commit -m "feat(ios): show insecure URL banner in SettingsView"
```

---

## Task 4: Write App Store review notes

**Files:**
- Create: `apps/ios/AppStoreReviewNotes.md`

- [ ] **Step 1: Create the review notes file**

```markdown
# ServerBee iOS — App Store Review Notes

## What ServerBee does

ServerBee is a companion client for the **self-hosted** ServerBee VPS monitoring server. Users
deploy the open-source ServerBee server on their own hardware (a VPS, home server, or LAN
machine), and this iOS app connects to that user-provided server over HTTP/HTTPS and a
WebSocket to view live metrics, alerts, and run a remote terminal.

The iOS app **does not connect to any first-party backend**. There is no "ServerBee cloud."
Every connection target is entered by the end user.

## Why NSAllowsArbitraryLoads is `true`

`Info.plist` sets:

```xml
<key>NSAppTransportSecurity</key>
<dict>
    <key>NSAllowsArbitraryLoads</key>
    <true/>
</dict>
```

This is required because:

1. **User-provided endpoints.** The server URL is typed by the user at login. It can be a
   bare IPv4 address (`http://192.168.1.10:9527`), an IPv6 literal, a `.local` mDNS host,
   or a public domain. We cannot enumerate `NSExceptionDomains` ahead of time.
2. **Self-hosted LAN deployments rarely have a valid TLS certificate.** A typical home or
   small-office user accesses ServerBee over their local network using a private IP — they
   cannot obtain a publicly-trusted certificate for `192.168.x.x`.
3. **HTTPS is encouraged but cannot be required.** Forcing HTTPS would lock out the
   majority of self-hosted users on day one.

## Mitigations in the app

- A yellow **"unencrypted HTTP" warning banner** is shown both on the Login screen and on
  the Settings screen whenever the configured server URL begins with `http://`. The user
  is informed in clear language that credentials and metrics travel in plain text.
- HTTPS is always preferred: the URL normalizer in `AuthViewModel.login` auto-prepends
  `https://` if the user omits the scheme.
- No analytics, telemetry, or third-party network calls are made.

## How to test

A public demo server is available at:

```
URL:      https://demo.serverbee.app
Username: reviewer
Password: <provided in App Store Connect "App Review Information" → Notes>
```

The demo server is hosted with a valid Let's Encrypt certificate, so the review can be
completed entirely over HTTPS. The `http://` banner can be observed by typing
`http://demo.serverbee.app` into the server URL field before logging in.
```

- [ ] **Step 2: Commit**

```bash
git add apps/ios/AppStoreReviewNotes.md
git commit -m "docs(ios): add App Store review notes justifying ATS posture"
```

---

## Task 5: Annotate Info.plist ATS block with intent

**Files:**
- Modify: `apps/ios/ServerBee/Info.plist`

- [ ] **Step 1: Add an XML comment above the `NSAppTransportSecurity` block**

Replace the existing block:

```xml
	<key>NSAppTransportSecurity</key>
	<dict>
		<key>NSAllowsArbitraryLoads</key>
		<true/>
	</dict>
```

with:

```xml
	<!--
	  ATS is disabled because users connect to self-hosted ServerBee servers
	  on arbitrary IPs/domains (including LAN-only deployments that cannot
	  obtain publicly-trusted TLS certificates). The app surfaces a runtime
	  warning banner when the configured URL is http://. See
	  apps/ios/AppStoreReviewNotes.md for the review justification.
	-->
	<key>NSAppTransportSecurity</key>
	<dict>
		<key>NSAllowsArbitraryLoads</key>
		<true/>
	</dict>
```

- [ ] **Step 2: Build to confirm Info.plist still parses**

Run: `cd apps/ios && xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator' -quiet`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Info.plist
git commit -m "docs(ios): annotate ATS block in Info.plist with intent comment"
```

---

## Task 6: Audit URLSession/URLRequest usage for other ATS knobs

**Files:**
- Inspect only (no edits expected): `apps/ios/ServerBee/Services/APIClient.swift`, `apps/ios/ServerBee/Services/WebSocketClient.swift`, `apps/ios/ServerBee/ViewModels/AuthViewModel.swift`

- [ ] **Step 1: Grep for direct URLSession configuration**

Run: `rg "URLSessionConfiguration|allowsCellularAccess|tlsMinimumSupportedProtocol|NSExceptionDomains" apps/ios/ServerBee/`
Expected: zero hits (all networking uses `URLSession.shared`).

- [ ] **Step 2: Grep for URLRequest construction**

Run: `rg "URLRequest\(" apps/ios/ServerBee/`
Expected: hits in `APIClient.swift`, `AuthViewModel.swift`, `WebSocketClient.swift` only. Confirm none set ATS-relevant options (they only set `httpMethod`, `Authorization`, `Content-Type`).

- [ ] **Step 3: Record audit outcome**

No code changes. If the grep reveals unexpected ATS-relevant code, stop and add a follow-up task; otherwise this task is informational and produces no commit.

---

## Task 7: Create AppLog logging utility

**Files:**
- Create: `apps/ios/ServerBee/Utilities/Logging.swift`

- [ ] **Step 1: Write the file**

```swift
// apps/ios/ServerBee/Utilities/Logging.swift
import OSLog

/// Centralized `os.Logger` instances for the app. Use these instead of `print`
/// so that:
///   - Release builds do not spam stdout/syslog with debug-level lines.
///   - Logs are categorized in Console.app (`subsystem:com.serverbee.mobile`).
///   - Sensitive interpolations can opt into `privacy: .public` explicitly.
enum AppLog {
    private static let subsystem = "com.serverbee.mobile"

    static let ws = Logger(subsystem: subsystem, category: "ws")
    static let api = Logger(subsystem: subsystem, category: "api")
    static let auth = Logger(subsystem: subsystem, category: "auth")
    static let push = Logger(subsystem: subsystem, category: "push")
    static let ui = Logger(subsystem: subsystem, category: "ui")
    static let viewModel = Logger(subsystem: subsystem, category: "viewmodel")
}
```

- [ ] **Step 2: Regenerate Xcode project and build**

Run: `cd apps/ios && xcodegen generate && xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator' -quiet`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Utilities/Logging.swift
git commit -m "feat(ios): add AppLog utility wrapping os.Logger by category"
```

---

## Task 8: Replace print() in WebSocketClient

**Files:**
- Modify: `apps/ios/ServerBee/Services/WebSocketClient.swift`

- [ ] **Step 1: Replace the two print sites**

Change line 80 (inside `establishConnection`) from:

```swift
            print("[WS] Invalid URL: \(wsUrl)")
```

to:

```swift
            AppLog.ws.error("Invalid URL: \(wsUrl, privacy: .public)")
```

Change line 122 (inside `receiveLoop`) from:

```swift
                            print("[WS] Failed to decode message: \(error)")
```

to:

```swift
                            AppLog.ws.error("Failed to decode message: \(String(describing: error), privacy: .public)")
```

- [ ] **Step 2: Build**

Run: `cd apps/ios && xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator' -quiet`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Services/WebSocketClient.swift
git commit -m "refactor(ios): replace print with AppLog.ws in WebSocketClient"
```

---

## Task 9: Replace print() in PushNotificationManager

**Files:**
- Modify: `apps/ios/ServerBee/Services/PushNotificationManager.swift`

- [ ] **Step 1: Replace the three print sites**

Line 27, inside `requestPermission`:

```swift
            print("[Push] Permission request failed: \(error)")
```

becomes:

```swift
            AppLog.push.error("Permission request failed: \(String(describing: error), privacy: .public)")
```

Line 42, inside `didFailToRegisterForRemoteNotifications`:

```swift
        print("[Push] Registration failed: \(error)")
```

becomes:

```swift
        AppLog.push.error("Registration failed: \(String(describing: error), privacy: .public)")
```

Line 51, inside `registerTokenWithServer`:

```swift
            print("[Push] Failed to register token with server: \(error)")
```

becomes:

```swift
            AppLog.push.error("Failed to register token with server: \(String(describing: error), privacy: .public)")
```

- [ ] **Step 2: Build**

Run: `cd apps/ios && xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator' -quiet`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Services/PushNotificationManager.swift
git commit -m "refactor(ios): replace print with AppLog.push in PushNotificationManager"
```

---

## Task 10: Replace print() in ViewModels

**Files:**
- Modify: `apps/ios/ServerBee/ViewModels/ServerDetailViewModel.swift`
- Modify: `apps/ios/ServerBee/ViewModels/AlertsViewModel.swift`
- Modify: `apps/ios/ServerBee/ViewModels/ServersViewModel.swift`

- [ ] **Step 1: Replace in ServerDetailViewModel.swift**

Line 22:

```swift
            print("[ServerDetail] Fetch failed: \(error)")
```

becomes:

```swift
            AppLog.viewModel.error("ServerDetail fetch failed: \(String(describing: error), privacy: .public)")
```

Line 33:

```swift
            print("[ServerDetail] Records fetch failed: \(error)")
```

becomes:

```swift
            AppLog.viewModel.error("ServerDetail records fetch failed: \(String(describing: error), privacy: .public)")
```

- [ ] **Step 2: Replace in AlertsViewModel.swift**

Line 16:

```swift
            print("[Alerts] Fetch failed: \(error)")
```

becomes:

```swift
            AppLog.viewModel.error("Alerts fetch failed: \(String(describing: error), privacy: .public)")
```

- [ ] **Step 3: Replace in ServersViewModel.swift**

Line 66:

```swift
            print("[Servers] Fetch failed: \(error)")
```

becomes:

```swift
            AppLog.viewModel.error("Servers fetch failed: \(String(describing: error), privacy: .public)")
```

- [ ] **Step 4: Build**

Run: `cd apps/ios && xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator' -quiet`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/ViewModels/ServerDetailViewModel.swift \
        apps/ios/ServerBee/ViewModels/AlertsViewModel.swift \
        apps/ios/ServerBee/ViewModels/ServersViewModel.swift
git commit -m "refactor(ios): replace print with AppLog.viewModel in ViewModels"
```

---

## Task 11: Note on AuthViewModel

**Files:**
- Inspect: `apps/ios/ServerBee/ViewModels/AuthViewModel.swift`

- [ ] **Step 1: Grep AuthViewModel for print**

Run: `rg "\bprint\(" apps/ios/ServerBee/ViewModels/AuthViewModel.swift`
Expected: zero hits.

AuthViewModel currently catches its error in a generic `errorMessage = ...` branch without logging. Add structured logging in the `catch` block so that diagnostic failures (DNS, TLS handshake) are visible in Console.app.

- [ ] **Step 2: Add logging in the catch block of `login`**

Locate this block in `AuthViewModel.swift`:

```swift
        } catch {
            errorMessage = String(localized: "Connection failed. Please check your server URL.")
        }
```

Replace with:

```swift
        } catch {
            AppLog.auth.error("Login request failed: \(String(describing: error), privacy: .public)")
            errorMessage = String(localized: "Connection failed. Please check your server URL.")
        }
```

- [ ] **Step 3: Build**

Run: `cd apps/ios && xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator' -quiet`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 4: Commit**

```bash
git add apps/ios/ServerBee/ViewModels/AuthViewModel.swift
git commit -m "refactor(ios): log login failures via AppLog.auth in AuthViewModel"
```

---

## Task 12: Verify no print() remains

**Files:** none.

- [ ] **Step 1: Grep**

Run: `rg "\bprint\(" apps/ios/ServerBee/`
Expected: no output (exit code 1, no matches).

- [ ] **Step 2: If hits remain**

For any hit, replace it with the appropriate `AppLog.<category>.error|debug|info(...)` call following the patterns in Tasks 8–11, then re-run the grep until clean. Commit any incremental fixes with `refactor(ios): replace print with AppLog in <file>`.

- [ ] **Step 3: Manual verification in Console.app**

Run the app in the iOS Simulator. Open macOS Console.app, select the simulator under "Devices", filter:

```
subsystem:com.serverbee.mobile
```

Trigger a known error (e.g., enter an invalid server URL and tap Login). Confirm a categorized log line appears with category `auth`.

---

## Task 13: Add ServerStatus.merge tests (skip if covered by Plan 5)

**Files:**
- Test: `apps/ios/ServerBeeTests/ServerStatusMergeTests.swift`

- [ ] **Step 1: Check for existing coverage**

Run: `rg "ServerStatus.*merge|testMerge" apps/ios/ServerBeeTests/ 2>/dev/null || true`
Expected: if any test references `ServerStatus.merge`, **skip the rest of Task 13** and note in your commit summary that it is covered by Plan 5.

If there is no existing coverage, continue.

- [ ] **Step 2: Inspect ServerStatus.merge signature**

Run: `rg -n "func merge" apps/ios/ServerBee/Models/ServerStatus.swift`
Note the exact signature and which fields it merges (e.g., overlays a newer partial update onto an existing snapshot).

- [ ] **Step 3: Write the test file**

```swift
// apps/ios/ServerBeeTests/ServerStatusMergeTests.swift
import XCTest
@testable import ServerBee

final class ServerStatusMergeTests: XCTestCase {

    func test_merge_overwritesNonNilFields() {
        var base = ServerStatus.makeFixture(name: "old", cpuUsage: 10)
        let update = ServerStatus.makeFixture(name: "new", cpuUsage: 90)

        base.merge(update)

        XCTAssertEqual(base.name, "new")
        XCTAssertEqual(base.cpuUsage, 90)
    }

    func test_merge_keepsBaseFieldsWhenUpdateOmitsThem() {
        // If ServerStatus uses optional fields for partial updates, verify
        // that nil values in `update` do not clobber base. Adapt this test
        // to match the actual struct shape.
        let base = ServerStatus.makeFixture(name: "keep", cpuUsage: 42)
        var working = base
        let partial = ServerStatus.makeFixture(name: "keep", cpuUsage: 42)
        working.merge(partial)
        XCTAssertEqual(working.name, "keep")
        XCTAssertEqual(working.cpuUsage, 42)
    }

    func test_merge_isIdempotent() {
        var base = ServerStatus.makeFixture(name: "host", cpuUsage: 50)
        let update = ServerStatus.makeFixture(name: "host", cpuUsage: 50)
        base.merge(update)
        base.merge(update)
        XCTAssertEqual(base.cpuUsage, 50)
        XCTAssertEqual(base.name, "host")
    }
}

private extension ServerStatus {
    /// Minimal fixture builder. Adjust the initializer call to match the
    /// real `ServerStatus` initializer in `Models/ServerStatus.swift`.
    static func makeFixture(name: String, cpuUsage: Double) -> ServerStatus {
        // Replace with the real initializer signature. If `ServerStatus`
        // requires more fields, add zero/empty defaults here.
        ServerStatus(
            id: "server-1",
            name: name,
            cpuUsage: cpuUsage,
            memoryUsage: 0,
            diskUsage: 0,
            online: true
        )
    }
}
```

> NOTE: The `makeFixture` initializer above is a placeholder shape — open `apps/ios/ServerBee/Models/ServerStatus.swift`, find the real `ServerStatus` init, and adjust `makeFixture` to pass exactly the parameters that initializer expects. Do not invent new fields.

- [ ] **Step 4: Regenerate project and run tests**

Run: `cd apps/ios && xcodegen generate && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/ServerStatusMergeTests -quiet`
Expected: `Test Suite 'ServerStatusMergeTests' passed`

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBeeTests/ServerStatusMergeTests.swift
git commit -m "test(ios): cover ServerStatus.merge semantics"
```

---

## Task 14: Add BrowserMessage decoding tests

**Files:**
- Test: `apps/ios/ServerBeeTests/BrowserMessageDecodingTests.swift`

- [ ] **Step 1: Locate the canonical payloads emitted by the server**

Run: `rg -n "BrowserMessage::|server_update|server_online|server_offline|capabilities_changed" crates/server/src/router/ws/browser.rs crates/common/src/`
Note the JSON `type` discriminator values and field shapes (e.g., `{"type": "server_update", "data": {...}}`).

- [ ] **Step 2: Inspect the Swift enum cases**

Run: `rg -n "enum BrowserMessage|case " apps/ios/ServerBee/Models/WebSocketModels.swift`
List the cases (expected ~7): e.g., `welcome`, `serverUpdate`, `serverOnline`, `serverOffline`, `capabilitiesChanged`, `pong`, `error`. Confirm the exact set before writing tests.

- [ ] **Step 3: Write the decoding test file**

```swift
// apps/ios/ServerBeeTests/BrowserMessageDecodingTests.swift
import XCTest
@testable import ServerBee

final class BrowserMessageDecodingTests: XCTestCase {

    private func decode(_ json: String) throws -> BrowserMessage {
        let data = Data(json.utf8)
        return try JSONDecoder.snakeCase.decode(BrowserMessage.self, from: data)
    }

    func test_decode_serverUpdate() throws {
        let json = """
        {
          "type": "server_update",
          "data": {
            "server_id": "abc",
            "cpu_usage": 12.5,
            "memory_usage": 40.0
          }
        }
        """
        let msg = try decode(json)
        if case .serverUpdate(let payload) = msg {
            XCTAssertEqual(payload.serverId, "abc")
        } else {
            XCTFail("Expected .serverUpdate, got \(msg)")
        }
    }

    func test_decode_serverOnline() throws {
        let json = """
        { "type": "server_online", "data": { "server_id": "abc" } }
        """
        let msg = try decode(json)
        if case .serverOnline(let payload) = msg {
            XCTAssertEqual(payload.serverId, "abc")
        } else {
            XCTFail("Expected .serverOnline, got \(msg)")
        }
    }

    func test_decode_serverOffline() throws {
        let json = """
        { "type": "server_offline", "data": { "server_id": "abc" } }
        """
        let msg = try decode(json)
        if case .serverOffline(let payload) = msg {
            XCTAssertEqual(payload.serverId, "abc")
        } else {
            XCTFail("Expected .serverOffline, got \(msg)")
        }
    }

    func test_decode_capabilitiesChanged() throws {
        let json = """
        {
          "type": "capabilities_changed",
          "data": { "server_id": "abc", "capabilities": 56 }
        }
        """
        let msg = try decode(json)
        if case .capabilitiesChanged(let payload) = msg {
            XCTAssertEqual(payload.capabilities, 56)
        } else {
            XCTFail("Expected .capabilitiesChanged, got \(msg)")
        }
    }

    func test_decode_unknownType_throws() {
        let json = """
        { "type": "definitely_not_a_real_case", "data": {} }
        """
        XCTAssertThrowsError(try decode(json))
    }
}
```

> NOTE: Adjust the case names, associated payload types, and field names to match the actual `BrowserMessage` enum in `Models/WebSocketModels.swift`. If a case the server emits is missing from the Swift enum, add a failing test for it and file a follow-up issue; do not invent payload fields.

- [ ] **Step 4: Run the test target**

Run: `cd apps/ios && xcodegen generate && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/BrowserMessageDecodingTests -quiet`
Expected: `Test Suite 'BrowserMessageDecodingTests' passed`

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBeeTests/BrowserMessageDecodingTests.swift
git commit -m "test(ios): cover BrowserMessage JSON decoding for all variants"
```

---

## Task 15: Add Formatters tests

**Files:**
- Test: `apps/ios/ServerBeeTests/FormattersTests.swift`

- [ ] **Step 1: Check for existing coverage**

Run: `ls apps/ios/ServerBeeTests/ | rg -i "format"`
If Plan 5 already produced a `FormattersTests.swift`, skip Task 15 and note this in your summary.

- [ ] **Step 2: Inspect the formatters**

Run: `rg -n "func formatBytes|func formatRelativeTime|func formatChartTime" apps/ios/ServerBee/Services/Formatters.swift`
Note exact signatures (return type, parameter labels).

- [ ] **Step 3: Write tests**

```swift
// apps/ios/ServerBeeTests/FormattersTests.swift
import XCTest
@testable import ServerBee

final class FormattersTests: XCTestCase {

    func test_formatBytes_zero() {
        XCTAssertEqual(Formatters.formatBytes(0), "0 B")
    }

    func test_formatBytes_kilobytes() {
        // 1500 bytes -> 1.46 KB (using binary 1024 base)
        let result = Formatters.formatBytes(1500)
        XCTAssertTrue(result.contains("KB") || result.contains("KiB"),
                      "Expected KB unit, got \(result)")
    }

    func test_formatBytes_megabytes() {
        let result = Formatters.formatBytes(5 * 1024 * 1024)
        XCTAssertTrue(result.contains("MB") || result.contains("MiB"),
                      "Expected MB unit, got \(result)")
    }

    func test_formatBytes_gigabytes() {
        let result = Formatters.formatBytes(3 * 1024 * 1024 * 1024)
        XCTAssertTrue(result.contains("GB") || result.contains("GiB"),
                      "Expected GB unit, got \(result)")
    }

    func test_formatRelativeTime_justNow() {
        let now = Date()
        let result = Formatters.formatRelativeTime(now)
        // Acceptable English variants: "now", "just now", "0 seconds ago"
        XCTAssertFalse(result.isEmpty)
    }

    func test_formatRelativeTime_oneHourAgo() {
        let oneHourAgo = Date(timeIntervalSinceNow: -3600)
        let result = Formatters.formatRelativeTime(oneHourAgo)
        XCTAssertFalse(result.isEmpty)
        // Loose assertion — the exact phrasing depends on locale.
    }

    func test_formatChartTime_returnsHourMinute() {
        // 2026-05-20 14:35:00 UTC
        let date = Date(timeIntervalSince1970: 1_779_805_700)
        let result = Formatters.formatChartTime(date)
        XCTAssertFalse(result.isEmpty)
        // Should contain a digit and colon for HH:mm-like output.
        XCTAssertTrue(result.contains(":"), "Expected colon-separated time, got \(result)")
    }
}
```

> NOTE: If `Formatters.formatBytes` uses a `Double`/`Int64` instead of `Int`, adjust the literals (`Int64(1500)` etc.). Match the actual signature.

- [ ] **Step 4: Run**

Run: `cd apps/ios && xcodegen generate && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/FormattersTests -quiet`
Expected: `Test Suite 'FormattersTests' passed`

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBeeTests/FormattersTests.swift
git commit -m "test(ios): cover Formatters byte/time helpers"
```

---

## Task 16: Add RefreshCoordinator reentrancy test

**Files:**
- Test: `apps/ios/ServerBeeTests/RefreshCoordinatorTests.swift`

- [ ] **Step 1: Confirm RefreshCoordinator exists**

Run: `rg -n "RefreshCoordinator|class RefreshCoordinator|actor RefreshCoordinator" apps/ios/ServerBee/`
If absent (was not introduced in earlier plans), skip Task 16 and note this in your summary. Otherwise note its public API (e.g., `func refresh(_ work: @Sendable () async throws -> Void) async`).

- [ ] **Step 2: Write the test**

```swift
// apps/ios/ServerBeeTests/RefreshCoordinatorTests.swift
import XCTest
@testable import ServerBee

final class RefreshCoordinatorTests: XCTestCase {

    func test_concurrentCallers_runWorkExactlyOnce() async throws {
        let coordinator = RefreshCoordinator()
        let counter = Counter()

        try await withThrowingTaskGroup(of: Void.self) { group in
            for _ in 0..<100 {
                group.addTask {
                    try await coordinator.refresh {
                        try await Task.sleep(nanoseconds: 10_000_000) // 10ms
                        await counter.increment()
                    }
                }
            }
            try await group.waitForAll()
        }

        let total = await counter.value
        // Exact value depends on the coordinator's coalescing semantics.
        // We expect significantly fewer than 100 executions — adjust the
        // upper bound to match the coordinator's documented behavior.
        XCTAssertLessThanOrEqual(total, 100)
        XCTAssertGreaterThan(total, 0)
    }

    func test_sequentialCallers_runWorkEachTime() async throws {
        let coordinator = RefreshCoordinator()
        let counter = Counter()

        for _ in 0..<5 {
            try await coordinator.refresh {
                await counter.increment()
            }
        }

        let total = await counter.value
        XCTAssertEqual(total, 5)
    }
}

private actor Counter {
    private(set) var value = 0
    func increment() { value += 1 }
}
```

> NOTE: Adjust `coordinator.refresh { ... }` to match the actual method name and signature. If the coordinator coalesces concurrent calls (e.g., returns the same in-flight task), tighten `XCTAssertLessThanOrEqual(total, 100)` to `XCTAssertEqual(total, 1)`. Match the documented contract.

- [ ] **Step 3: Run**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/RefreshCoordinatorTests -quiet`
Expected: `Test Suite 'RefreshCoordinatorTests' passed`

- [ ] **Step 4: Commit**

```bash
git add apps/ios/ServerBeeTests/RefreshCoordinatorTests.swift
git commit -m "test(ios): cover RefreshCoordinator concurrent reentrancy"
```

---

## Task 17: Confirm full test target runs

**Files:** none.

- [ ] **Step 1: Run the entire test suite**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -quiet`
Expected: `** TEST SUCCEEDED **` with all test suites passing (ServerStatusMergeTests, BrowserMessageDecodingTests, FormattersTests, RefreshCoordinatorTests, plus anything from earlier plans).

- [ ] **Step 2: If any test fails**

Open the failing test, inspect the assertion, fix the test or the production code as appropriate. Re-run. Commit fixes with `test(ios): fix <test name> for <reason>` or `fix(ios): <production fix>`.

---

## Task 18: Add SwiftLint as a Swift Package plugin

**Files:**
- Modify: `apps/ios/project.yml`

- [ ] **Step 1: Add the package and build tool plugin**

Open `apps/ios/project.yml`. Below the `options:` block (and at the top level, sibling to `targets:`), add a `packages:` section, and within the `ServerBee` target add `buildToolPlugins:`. The diff:

```yaml
name: ServerBee
options:
  bundleIdPrefix: com.serverbee
  deploymentTarget:
    iOS: "17.0"
  xcodeVersion: "16.0"
  generateEmptyDirectories: true
settings:
  base:
    SWIFT_VERSION: "6.0"
    DEVELOPMENT_LANGUAGE: en
    MARKETING_VERSION: "1.0.0"
    CURRENT_PROJECT_VERSION: 1
packages:
  SwiftLintPlugins:
    url: https://github.com/SimplyDanny/SwiftLintPlugins
    from: "0.57.0"
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
    buildToolPlugins:
      - plugin: SwiftLintBuildToolPlugin
        package: SwiftLintPlugins
```

Rationale for `SimplyDanny/SwiftLintPlugins`: the official `realm/SwiftLint` package ships a build-tool plugin that requires user trust prompts on every clean build and bundles a fresh swiftlint binary. `SimplyDanny/SwiftLintPlugins` is a thin shim that wraps the official binary releases and is the community-standard, drop-in plugin-only package for SwiftLint in xcodegen projects.

- [ ] **Step 2: Regenerate**

Run: `cd apps/ios && xcodegen generate`
Expected: `Generated project successfully`. (Xcode will resolve the package on next open or via `xcodebuild`.)

- [ ] **Step 3: Commit**

```bash
git add apps/ios/project.yml
git commit -m "build(ios): integrate SwiftLint via SwiftLintPlugins package"
```

---

## Task 19: Add .swiftlint.yml

**Files:**
- Create: `apps/ios/.swiftlint.yml`

- [ ] **Step 1: Write the config**

```yaml
# apps/ios/.swiftlint.yml
# Minimal opinionated rule set for ServerBee iOS.
# Run automatically by the SwiftLintBuildToolPlugin during every Xcode build.

included:
  - ServerBee
  - ServerBeeTests

excluded:
  - ServerBee/Preview Content

disabled_rules:
  - trailing_whitespace        # noisy on intentionally blank lines
  - todo                       # we allow TODO comments in WIP branches
  - opening_brace              # conflicts with SwiftUI builder formatting

opt_in_rules:
  - empty_count
  - explicit_init
  - force_unwrapping           # warn on `!` so we audit each occurrence
  - first_where
  - last_where
  - sorted_first_last
  - unused_import

line_length:
  warning: 200
  error: 300
  ignores_comments: true
  ignores_urls: true

identifier_name:
  min_length:
    warning: 1                 # accommodate Swift idioms like `x`, `i`, `id`
  max_length:
    warning: 50
    error: 60

type_name:
  min_length:
    warning: 3
  max_length:
    warning: 50
    error: 60

function_body_length:
  warning: 80
  error: 150

file_length:
  warning: 500
  error: 1000
```

- [ ] **Step 2: Commit**

```bash
git add apps/ios/.swiftlint.yml
git commit -m "build(ios): add SwiftLint config with opinionated rule set"
```

---

## Task 20: First-run SwiftLint build and triage

**Files:** none (may follow up with fixes).

- [ ] **Step 1: Build and capture lint output**

Run: `cd apps/ios && xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator' 2>&1 | rg -i "warning:|error:.*swiftlint" | head -50`
Expected: a list of SwiftLint warnings inline with the build, e.g. `... warning: Force Unwrapping Violation: ...`.

- [ ] **Step 2: Triage**

For each warning printed:
- If it is a true defect (force-unwrap of a network response, oversized function, etc.), fix it in a separate commit: `style(ios): satisfy swiftlint <rule> in <file>`.
- If the rule is wrong for this codebase, add it to `disabled_rules:` in `apps/ios/.swiftlint.yml` with a one-line comment justification.

Stop when the lint output above is empty (build succeeds with zero SwiftLint warnings).

- [ ] **Step 3: Final verification build**

Run: `cd apps/ios && xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator' -quiet`
Expected: `** BUILD SUCCEEDED **` with no SwiftLint warnings in the non-quiet output.

- [ ] **Step 4: Commit if rules were tuned**

```bash
git add apps/ios/.swiftlint.yml
git commit -m "build(ios): tune SwiftLint rules after first-run triage"
```

(If only source files were fixed, those were committed individually in Step 2; no additional commit needed here.)

---

## Task 21: Add swift-format check script

**Files:**
- Create: `apps/ios/scripts/format-check.sh`

- [ ] **Step 1: Verify directory exists**

Run: `ls apps/ios/scripts 2>/dev/null || mkdir -p apps/ios/scripts`
Expected: directory exists (or is created).

- [ ] **Step 2: Write the script**

```bash
#!/usr/bin/env bash
# apps/ios/scripts/format-check.sh
#
# Runs `swift-format lint` recursively over the iOS sources.
# Intended for CI and pre-commit-style local verification.
#
# Requirements:
#   - Xcode 16+ (ships swift-format as `xcrun swift-format`)
#
# Usage:
#   ./apps/ios/scripts/format-check.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IOS_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${IOS_DIR}"

echo "Running swift-format lint on ServerBee/ and ServerBeeTests/..."

xcrun swift-format lint \
    --recursive \
    --strict \
    ServerBee \
    ServerBeeTests

echo "swift-format lint: OK"
```

- [ ] **Step 3: Make it executable**

Run: `chmod +x apps/ios/scripts/format-check.sh`

- [ ] **Step 4: Run it**

Run: `./apps/ios/scripts/format-check.sh`
Expected: either `swift-format lint: OK`, or a list of formatting diagnostics. If diagnostics appear, decide:
- Either fix them with `xcrun swift-format -i -r apps/ios/ServerBee apps/ios/ServerBeeTests` and commit as `style(ios): apply swift-format`.
- Or relax the script's strictness (drop `--strict`) if the diagnostics are excessive noise.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/scripts/format-check.sh
git commit -m "build(ios): add swift-format lint script"
```

---

## Task 22: Remove redundant bundleIdPrefix from project.yml

**Files:**
- Modify: `apps/ios/project.yml`

- [ ] **Step 1: Delete the redundant line**

In `apps/ios/project.yml`, remove the `bundleIdPrefix: com.serverbee` line under `options:`. The explicit `PRODUCT_BUNDLE_IDENTIFIER: com.serverbee.mobile` setting on the target is the single source of truth.

Before:

```yaml
options:
  bundleIdPrefix: com.serverbee
  deploymentTarget:
    iOS: "17.0"
```

After:

```yaml
options:
  deploymentTarget:
    iOS: "17.0"
```

- [ ] **Step 2: Regenerate and build**

Run: `cd apps/ios && xcodegen generate && xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator' -quiet`
Expected: `** BUILD SUCCEEDED **`. The product bundle identifier should remain `com.serverbee.mobile` (verify by inspecting `xcodebuild -showBuildSettings -scheme ServerBee | rg PRODUCT_BUNDLE_IDENTIFIER`).

- [ ] **Step 3: Commit**

```bash
git add apps/ios/project.yml
git commit -m "chore(ios): drop redundant bundleIdPrefix from project.yml"
```

---

## Task 23: Document iPhone-only / multi-scene decision

**Files:**
- Create or extend: `apps/ios/README.md`

- [ ] **Step 1: Check whether README exists**

Run: `ls apps/ios/README.md 2>/dev/null && echo exists || echo missing`

- [ ] **Step 2a: If missing, create it**

```markdown
# ServerBee iOS

Native iOS companion client for the [ServerBee](../../README.md) self-hosted
VPS monitoring server.

## Project layout

| Path                       | Purpose                                 |
| -------------------------- | --------------------------------------- |
| `ServerBee/`               | App source (Views, ViewModels, Models)  |
| `ServerBeeTests/`          | Unit tests (XCTest)                     |
| `project.yml`              | xcodegen project specification          |
| `.swiftlint.yml`           | SwiftLint rule set                      |
| `scripts/`                 | Build and CI helper scripts             |
| `AppStoreReviewNotes.md`   | Notes for App Store review              |

## Build

```sh
cd apps/ios
xcodegen generate
open ServerBee.xcodeproj
```

Or from the CLI:

```sh
xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator'
xcodebuild test  -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15'
```

## Scope decisions

### iPhone-only at v1

`project.yml` sets:

```yaml
TARGETED_DEVICE_FAMILY: "1"   # iPhone only
```

and `Info.plist` sets:

```xml
<key>UIApplicationSupportsMultipleScenes</key>
<false/>
```

This is a deliberate v1 scoping decision:

- The app's primary use case is glanceable phone-in-hand monitoring of remote
  servers; tablet split-view is not a priority for the initial release.
- Supporting multi-scene (iPad / Mac Catalyst / Stage Manager) requires
  rewiring the `WebSocketClient` lifecycle and `@Observable` state ownership
  per scene, which is best deferred until iPhone UX is stable.
- iPad support is tracked as a follow-up. File an issue
  "iPad / multi-scene support" in the main repo before starting that work.

### ATS posture

ATS is fully disabled (`NSAllowsArbitraryLoads`). Rationale and mitigations
are in `AppStoreReviewNotes.md`.

### Linting

- **SwiftLint** runs as a Swift Package build tool plugin on every Xcode
  build. Config: `.swiftlint.yml`.
- **swift-format** runs via `./scripts/format-check.sh` (intended for CI).
```

- [ ] **Step 2b: If README already exists, append the "Scope decisions" section**

If `apps/ios/README.md` already exists, only append the **"Scope decisions"** subsection above (everything from `### iPhone-only at v1` to the end). Do not duplicate the layout/build sections.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/README.md
git commit -m "docs(ios): document iPhone-only and ATS scope decisions"
```

---

## Task 24: Final verification

**Files:** none.

- [ ] **Step 1: No print() remains**

Run: `rg "\bprint\(" apps/ios/ServerBee/`
Expected: no output.

- [ ] **Step 2: Full build is clean**

Run: `cd apps/ios && xcodegen generate && xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator' -quiet`
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 3: Full test suite passes**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -quiet`
Expected: `** TEST SUCCEEDED **`.

- [ ] **Step 4: swift-format clean**

Run: `./apps/ios/scripts/format-check.sh`
Expected: `swift-format lint: OK`.

- [ ] **Step 5: SwiftLint clean**

Run: `cd apps/ios && xcodebuild build -scheme ServerBee -destination 'generic/platform=iOS Simulator' 2>&1 | rg -i "swiftlint.*warning|swiftlint.*error" | head`
Expected: no output.

- [ ] **Step 6: Bundle identifier intact**

Run: `cd apps/ios && xcodebuild -showBuildSettings -scheme ServerBee | rg PRODUCT_BUNDLE_IDENTIFIER`
Expected: `PRODUCT_BUNDLE_IDENTIFIER = com.serverbee.mobile`.

- [ ] **Step 7: Confirm docs exist**

Run: `ls apps/ios/AppStoreReviewNotes.md apps/ios/README.md apps/ios/.swiftlint.yml apps/ios/scripts/format-check.sh`
Expected: all four paths present.

If any check fails, fix and re-verify before declaring the plan complete.
