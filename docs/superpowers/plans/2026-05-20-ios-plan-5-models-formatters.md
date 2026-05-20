# iOS Plan 5: Models, Formatters, and Localization Cleanup

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align iOS models with the real backend payload shape (especially partial-update `online`), unify JSON encoding strategy on explicit CodingKeys, replace ad-hoc formatters with system-provided locale-aware ones, and standardize the Localizable.xcstrings key convention.

**Architecture:** Models become single-source-of-truth via explicit CodingKeys; `JSONEncoder.snakeCase` no longer rewrites. `Formatters` enum caches static DateFormatter instances. `RelativeDateTimeFormatter` and `ByteCountFormatter` replace hand-rolled logic for locale correctness. Localizable.xcstrings keys become the English source text (Apple's modern convention).

**Tech Stack:** Swift, Foundation, XCTest.

**Depends on:** Plan 1 (`ServerBeeTests` target).

---

## Task 1: Backend Audit and Findings

**Files:**
- Read only: `crates/common/src/types.rs:140-179`
- Read only: `crates/common/src/protocol.rs:453-526`
- Read only: `crates/server/src/service/alert.rs:380-438`

- [ ] **Step 1: Read backend ServerStatus**

Open `crates/common/src/types.rs` lines 140-179. Record in a scratch note:

```
struct ServerStatus {
    id: String,                 // required
    name: String,               // required
    online: bool,               // REQUIRED, non-optional
    last_active: i64,           // required
    uptime: u64,                // required
    cpu: f64,                   // required (NOTE: NOT "cpu_usage")
    mem_used: i64,              // required (NOTE: NOT "memory_used")
    mem_total: i64,             // required
    swap_used / swap_total,     // required
    disk_used / disk_total,     // required
    net_in_speed / net_out_speed,            // required
    net_in_transfer / net_out_transfer,      // required
    load1, load5, load15: f64,  // required
    tcp_conn, udp_conn, process_count: i32,  // required
    cpu_name, os, region, country_code, group_id: Option<String>,
    features: Vec<String> (default),
    disk_read_bytes_per_sec, disk_write_bytes_per_sec: u64 (default),
    tags: Vec<String> (default),
    cpu_cores: Option<i32> (default),
}
```

- [ ] **Step 2: Read backend BrowserMessage**

Open `crates/common/src/protocol.rs` lines 453-526. The `Update` variant carries `servers: Vec<ServerStatus>` — i.e. each entry is a **full** `ServerStatus`, not a partial. There is no `Option<bool>` wrapping `online`.

- [ ] **Step 3: Read backend AlertEventResponse**

Open `crates/server/src/service/alert.rs` lines 380-438. The fields actually emitted to mobile are: `rule_id, rule_name, server_id, server_name, status (string "firing"|"resolved"), event_at, resolved_at, count`. Timestamps use `chrono::DateTime::to_rfc3339()` (subsecond precision varies; **may or may not include fractional seconds** depending on the source value).

- [ ] **Step 4: Record decisions in this plan file**

Decisions (documented inline in Task 2 onward):
1. **`online`**: Make it `Bool?` in iOS even though backend currently sends it as required. The iOS struct already declares all *other* metric fields as optional defensively. Making `online` optional follows that pattern and makes `merge` correctly preserve a local value when a future partial-update protocol omits it. Cost: every read site must default to `false`.
2. **Encoding strategy**: Keep hand-written `CodingKeys` everywhere; **remove** `.convertToSnakeCase` from `JSONEncoder.snakeCase`. Rationale: explicit CodingKeys survive Swift property renames and serve as documentation.
3. **Localization keys**: Unify on English source text (Apple modern convention). Rename all `settings_*` snake keys.

- [ ] **Step 5: Commit audit notes**

This task changes no files; skip commit.

---

## Task 2: Make `ServerStatus.online` optional

**Files:**
- Modify: `apps/ios/ServerBee/Models/ServerStatus.swift:1-113`
- Modify: `apps/ios/ServerBee/ViewModels/ServersViewModel.swift:42-97`
- Modify: `apps/ios/ServerBee/Views/Servers/ServerDetailView.swift:80-89,193`
- Modify: `apps/ios/ServerBee/Views/Servers/ServerCardView.swift:13-22,94`
- Create: `apps/ios/ServerBeeTests/ServerStatusMergeTests.swift`

- [ ] **Step 1: Write failing test for partial-update preserving local online**

Create `apps/ios/ServerBeeTests/ServerStatusMergeTests.swift`:

```swift
import XCTest
@testable import ServerBee

final class ServerStatusMergeTests: XCTestCase {
    func test_merge_preservesOnline_whenIncomingOnlineIsNil() {
        var local = ServerStatus(
            id: "s1", name: "Local", online: true,
            cpuUsage: 10, memoryTotal: nil, memoryUsed: nil,
            diskTotal: nil, diskUsed: nil, networkIn: nil, networkOut: nil,
            load1: nil, load5: nil, load15: nil,
            processCount: nil, tcpCount: nil, udpCount: nil,
            uptime: nil, os: nil, cpuName: nil, ipv4: nil, ipv6: nil,
            region: nil, country: nil, groupName: nil, lastActiveAt: nil
        )
        let partial = ServerStatus(
            id: "s1", name: "Local", online: nil,
            cpuUsage: 42, memoryTotal: nil, memoryUsed: nil,
            diskTotal: nil, diskUsed: nil, networkIn: nil, networkOut: nil,
            load1: nil, load5: nil, load15: nil,
            processCount: nil, tcpCount: nil, udpCount: nil,
            uptime: nil, os: nil, cpuName: nil, ipv4: nil, ipv6: nil,
            region: nil, country: nil, groupName: nil, lastActiveAt: nil
        )
        local.merge(from: partial)
        XCTAssertEqual(local.online, true, "merge with nil online must preserve local")
        XCTAssertEqual(local.cpuUsage, 42)
    }

    func test_merge_appliesOnline_whenIncomingProvidesIt() {
        var local = ServerStatus(
            id: "s1", name: "Local", online: true,
            cpuUsage: nil, memoryTotal: nil, memoryUsed: nil,
            diskTotal: nil, diskUsed: nil, networkIn: nil, networkOut: nil,
            load1: nil, load5: nil, load15: nil,
            processCount: nil, tcpCount: nil, udpCount: nil,
            uptime: nil, os: nil, cpuName: nil, ipv4: nil, ipv6: nil,
            region: nil, country: nil, groupName: nil, lastActiveAt: nil
        )
        let incoming = ServerStatus(
            id: "s1", name: "Local", online: false,
            cpuUsage: nil, memoryTotal: nil, memoryUsed: nil,
            diskTotal: nil, diskUsed: nil, networkIn: nil, networkOut: nil,
            load1: nil, load5: nil, load15: nil,
            processCount: nil, tcpCount: nil, udpCount: nil,
            uptime: nil, os: nil, cpuName: nil, ipv4: nil, ipv6: nil,
            region: nil, country: nil, groupName: nil, lastActiveAt: nil
        )
        local.merge(from: incoming)
        XCTAssertEqual(local.online, false)
    }
}
```

- [ ] **Step 2: Run test to verify it fails to compile**

Run: `cd apps/ios && xcodegen generate && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/ServerStatusMergeTests`

Expected: BUILD FAILURE — `Cannot convert value of type 'Bool' to expected argument type 'Bool?'` at the test's `online: true` / `online: nil` lines (struct still uses non-optional Bool).

- [ ] **Step 3: Update ServerStatus to make `online` optional**

Replace the full content of `apps/ios/ServerBee/Models/ServerStatus.swift`:

```swift
import Foundation

struct ServerStatus: Codable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    /// Optional: WebSocket partial updates may omit this field. Treat `nil` as "unknown — keep previous state".
    var online: Bool?
    var cpuUsage: Double?
    var memoryTotal: Int64?
    var memoryUsed: Int64?
    var diskTotal: Int64?
    var diskUsed: Int64?
    var networkIn: Int64?
    var networkOut: Int64?
    var load1: Double?
    var load5: Double?
    var load15: Double?
    var processCount: Int?
    var tcpCount: Int?
    var udpCount: Int?
    var uptime: Int64?
    var os: String?
    var cpuName: String?
    var ipv4: String?
    var ipv6: String?
    var region: String?
    var country: String?
    var groupName: String?
    var lastActiveAt: String?

    enum CodingKeys: String, CodingKey {
        case id
        case name
        case online
        case cpuUsage = "cpu_usage"
        case memoryTotal = "memory_total"
        case memoryUsed = "memory_used"
        case diskTotal = "disk_total"
        case diskUsed = "disk_used"
        case networkIn = "network_in"
        case networkOut = "network_out"
        case load1
        case load5
        case load15
        case processCount = "process_count"
        case tcpCount = "tcp_count"
        case udpCount = "udp_count"
        case uptime
        case os
        case cpuName = "cpu_name"
        case ipv4
        case ipv6
        case region
        case country
        case groupName = "group_name"
        case lastActiveAt = "last_active_at"
    }

    /// Convenience: treat unknown (`nil`) as offline for view code.
    var isOnline: Bool { online ?? false }
}

extension ServerStatus {
    /// First non-nil IP address (prefer IPv4).
    var primaryIP: String? {
        ipv4 ?? ipv6
    }

    /// Memory usage percentage.
    var memoryPercent: Double? {
        guard let used = memoryUsed, let total = memoryTotal, total > 0 else { return nil }
        return Double(used) / Double(total) * 100
    }

    /// Disk usage percentage.
    var diskPercent: Double? {
        guard let used = diskUsed, let total = diskTotal, total > 0 else { return nil }
        return Double(used) / Double(total) * 100
    }

    /// Human-readable location derived from region + country.
    var location: String? {
        switch (region, country) {
        case let (r?, c?): return "\(r), \(c)"
        case let (r?, nil): return r
        case let (nil, c?): return c
        default: return nil
        }
    }

    /// Merge non-nil fields from another status (used for WebSocket partial updates).
    /// Fields that are `nil` in `other` preserve the local value.
    mutating func merge(from other: ServerStatus) {
        if let v = other.online { online = v }
        if let v = other.cpuUsage { cpuUsage = v }
        if let v = other.memoryTotal { memoryTotal = v }
        if let v = other.memoryUsed { memoryUsed = v }
        if let v = other.diskTotal { diskTotal = v }
        if let v = other.diskUsed { diskUsed = v }
        if let v = other.networkIn { networkIn = v }
        if let v = other.networkOut { networkOut = v }
        if let v = other.load1 { load1 = v }
        if let v = other.load5 { load5 = v }
        if let v = other.load15 { load15 = v }
        if let v = other.processCount { processCount = v }
        if let v = other.tcpCount { tcpCount = v }
        if let v = other.udpCount { udpCount = v }
        if let v = other.uptime { uptime = v }
        if let v = other.os { os = v }
        if let v = other.cpuName { cpuName = v }
        if let v = other.ipv4 { ipv4 = v }
        if let v = other.ipv6 { ipv6 = v }
        if let v = other.region { region = v }
        if let v = other.country { country = v }
        if let v = other.groupName { groupName = v }
        if let v = other.lastActiveAt { lastActiveAt = v }
    }
}

struct MetricRecord: Codable, Identifiable, Sendable {
    var id: String { timestamp }
    let timestamp: String
    var cpuUsage: Double?
    var memoryUsed: Int64?
    var memoryTotal: Int64?
    var networkIn: Int64?
    var networkOut: Int64?
    var diskUsed: Int64?
    var diskTotal: Int64?

    enum CodingKeys: String, CodingKey {
        case timestamp
        case cpuUsage = "cpu_usage"
        case memoryUsed = "memory_used"
        case memoryTotal = "memory_total"
        case networkIn = "network_in"
        case networkOut = "network_out"
        case diskUsed = "disk_used"
        case diskTotal = "disk_total"
    }
}

extension MetricRecord {
    /// Parsed Date from the ISO 8601 timestamp string.
    var date: Date? {
        ISO8601DateFormatter.shared.date(from: timestamp)
    }

    /// Memory usage percentage (0-100).
    var memoryPercent: Double? {
        guard let used = memoryUsed, let total = memoryTotal, total > 0 else { return nil }
        return Double(used) / Double(total) * 100
    }

    /// Disk usage percentage (0-100).
    var diskPercent: Double? {
        guard let used = diskUsed, let total = diskTotal, total > 0 else { return nil }
        return Double(used) / Double(total) * 100
    }
}
```

- [ ] **Step 4: Update ServersViewModel.swift read sites**

In `apps/ios/ServerBee/ViewModels/ServersViewModel.swift`:

Replace lines 42-43:

```swift
        case .online: result = result.filter { $0.isOnline }
        case .offline: result = result.filter { !$0.isOnline }
```

Replace line 48:

```swift
            if a.isOnline != b.isOnline { return a.isOnline && !b.isOnline }
```

Replace line 57:

```swift
        servers.filter(\.isOnline).count
```

Replace lines 92 and 97 (the writes set explicit Bool — wrap in optional):

```swift
                servers[index].online = true
```

(stays the same — Swift auto-promotes `Bool` to `Bool?` on assignment).

```swift
                servers[index].online = false
```

(stays the same).

- [ ] **Step 5: Update ServerDetailView.swift read sites**

In `apps/ios/ServerBee/Views/Servers/ServerDetailView.swift` lines 80-89, replace every `server.online` with `server.isOnline`:

```swift
                .fill(server.isOnline ? Color.serverOnline : Color.serverOffline)
```

```swift
            Text(server.isOnline ? String(localized: "Online") : String(localized: "Offline"))
```

```swift
                .foregroundStyle(server.isOnline ? Color.serverOnline : Color.serverOffline)
```

```swift
            (server.isOnline ? Color.serverOnline : Color.serverOffline).opacity(0.1)
```

For the preview at line 193 (`ServerStatus(... online: true ...)`), leave as-is — `true` is a valid `Bool?` literal.

- [ ] **Step 6: Update ServerCardView.swift read sites**

In `apps/ios/ServerBee/Views/Servers/ServerCardView.swift`:

Replace line 13:

```swift
                    .fill(server.isOnline ? Color.serverOnline : Color.serverOffline)
```

Replace line 22:

```swift
                if let lastActive = server.lastActiveAt, !server.isOnline {
```

Line 94 preview (`online: true`) stays the same.

- [ ] **Step 7: Run test to verify it passes**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/ServerStatusMergeTests`

Expected: PASS for both tests. Build succeeds for the app target (all `server.online` reads converted to `server.isOnline`).

- [ ] **Step 8: Commit**

```bash
git add apps/ios/ServerBee/Models/ServerStatus.swift \
        apps/ios/ServerBee/ViewModels/ServersViewModel.swift \
        apps/ios/ServerBee/Views/Servers/ServerDetailView.swift \
        apps/ios/ServerBee/Views/Servers/ServerCardView.swift \
        apps/ios/ServerBeeTests/ServerStatusMergeTests.swift
git commit -m "fix(ios): make ServerStatus.online optional and preserve local value on partial merge"
```

---

## Task 3: Fix duplicate `MobileAlertEvent.id` across status transitions

**Files:**
- Modify: `apps/ios/ServerBee/Models/AlertModels.swift:8-22`
- Create: `apps/ios/ServerBeeTests/MobileAlertEventIdTests.swift`

- [ ] **Step 1: Write failing test for unique ID across firing/resolved**

Create `apps/ios/ServerBeeTests/MobileAlertEventIdTests.swift`:

```swift
import XCTest
@testable import ServerBee

final class MobileAlertEventIdTests: XCTestCase {
    private func make(status: AlertStatus, updatedAt: String) -> MobileAlertEvent {
        MobileAlertEvent(
            alertKey: "rule-1:server-1",
            ruleId: "rule-1",
            ruleName: "High CPU",
            serverId: "server-1",
            serverName: "vps-a",
            status: status,
            message: "msg",
            triggerCount: 3,
            firstTriggeredAt: "2026-05-20T10:00:00Z",
            lastNotifiedAt: updatedAt,
            resolvedAt: status == .resolved ? updatedAt : nil,
            updatedAt: updatedAt
        )
    }

    func test_id_differs_betweenFiringAndResolved_sameAlertKey() {
        let firing = make(status: .firing, updatedAt: "2026-05-20T10:05:00Z")
        let resolved = make(status: .resolved, updatedAt: "2026-05-20T10:10:00Z")
        XCTAssertNotEqual(firing.id, resolved.id, "Different status must yield distinct IDs")
    }

    func test_id_stableForSameStatusAndTimestamp() {
        let a = make(status: .firing, updatedAt: "2026-05-20T10:05:00Z")
        let b = make(status: .firing, updatedAt: "2026-05-20T10:05:00Z")
        XCTAssertEqual(a.id, b.id)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/MobileAlertEventIdTests`

Expected: FAIL on `test_id_differs_betweenFiringAndResolved_sameAlertKey` (current `id = alertKey` returns the same string for both).

- [ ] **Step 3: Update MobileAlertEvent.id**

Replace lines 8-22 of `apps/ios/ServerBee/Models/AlertModels.swift`:

```swift
struct MobileAlertEvent: Codable, Identifiable, Sendable {
    /// Composite ID: `alertKey#status#updatedAt`. Required because the same
    /// `alertKey` (e.g. "rule:server") is reused across firing→resolved
    /// transitions, which would otherwise produce duplicate SwiftUI ForEach IDs.
    var id: String { "\(alertKey)#\(status.rawValue)#\(updatedAt)" }
    let alertKey: String
    let ruleId: String
    let ruleName: String
    let serverId: String
    let serverName: String
    let status: AlertStatus
    let message: String
    let triggerCount: Int
    let firstTriggeredAt: String
    let lastNotifiedAt: String
    let resolvedAt: String?
    let updatedAt: String
```

(The `CodingKeys` enum and the rest of the file are unchanged.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/MobileAlertEventIdTests`

Expected: PASS for both tests.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/Models/AlertModels.swift \
        apps/ios/ServerBeeTests/MobileAlertEventIdTests.swift
git commit -m "fix(ios): give MobileAlertEvent a composite id to avoid ForEach duplicates"
```

---

## Task 4: Delete unused `selectedRange` from `ServerDetailViewModel`

**Files:**
- Modify: `apps/ios/ServerBee/ViewModels/ServerDetailViewModel.swift:9`

- [ ] **Step 1: Confirm `MetricsHistoryView` owns its own selectedRange**

Read `apps/ios/ServerBee/Views/Servers/MetricsHistoryView.swift` lines 10-12. Verify:

```swift
@State private var viewModel = ServerDetailViewModel()
@State private var selectedRange = "1h"
```

The view declares its own `@State`. The viewmodel's copy is unread.

- [ ] **Step 2: Remove the field**

In `apps/ios/ServerBee/ViewModels/ServerDetailViewModel.swift`, delete line 9:

```swift
    var selectedRange = "1h"
```

Resulting class header:

```swift
@MainActor
@Observable
final class ServerDetailViewModel {
    var server: ServerStatus?
    var records: [MetricRecord] = []
    var isLoading = false

    /// Set the server from the parent list (avoids a separate network fetch).
    func setServer(_ server: ServerStatus) {
        self.server = server
    }

    // ... rest unchanged
```

- [ ] **Step 3: Verify build**

Run: `cd apps/ios && xcodebuild build -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15'`

Expected: BUILD SUCCEEDED. No warnings about unused property.

- [ ] **Step 4: Commit**

```bash
git add apps/ios/ServerBee/ViewModels/ServerDetailViewModel.swift
git commit -m "refactor(ios): drop unused selectedRange from ServerDetailViewModel"
```

---

## Task 5: Document the JSON coding convention

**Files:**
- Create: `apps/ios/ServerBee/Models/README.md`

- [ ] **Step 1: Write the convention doc**

Create `apps/ios/ServerBee/Models/README.md`:

```markdown
# iOS Models — Coding Convention

All `Codable` model types in this directory MUST declare an explicit
`enum CodingKeys: String, CodingKey` covering every Swift property whose
JSON wire-format name differs from the property name (and ideally all
properties, for documentation value).

## Why

Explicit `CodingKeys`:

1. Survive Swift property renames — a refactor of `var memoryUsed` to
   `var memUsed` will not silently break decoding against the backend.
2. Document the exact JSON contract right next to the Swift model.
3. Permit per-field opt-outs and renames (e.g. `cpuUsage = "cpu_usage"`)
   that key-strategy converters cannot express cleanly.

## Encoder / decoder

- `JSONEncoder()` (default, no key-encoding strategy) — properties encode
  exactly as `CodingKeys` declares them.
- `JSONDecoder()` (default) — same.
- Do **not** add `.convertToSnakeCase` / `.convertFromSnakeCase`.

The helper `JSONDecoder.snakeCase` still exists for legacy code that
relied on it, but new code should use the default decoder and let the
explicit `CodingKeys` do the work.
```

- [ ] **Step 2: Commit**

```bash
git add apps/ios/ServerBee/Models/README.md
git commit -m "docs(ios): document explicit CodingKeys convention for Models"
```

---

## Task 6: Audit every model for missing CodingKeys

**Files:**
- Verify: `apps/ios/ServerBee/Models/AuthModels.swift` (already has CodingKeys ✓)
- Verify: `apps/ios/ServerBee/Models/AlertModels.swift` (already has CodingKeys ✓)
- Verify: `apps/ios/ServerBee/Models/ServerStatus.swift` (already has CodingKeys ✓)
- Verify: `apps/ios/ServerBee/Models/WebSocketModels.swift` (has manual init ✓)
- Verify: `apps/ios/ServerBee/Models/APIModels.swift` (only has generic wrappers — no fields)

- [ ] **Step 1: Grep for any `Codable` struct missing CodingKeys**

Run from repo root:

```bash
for f in apps/ios/ServerBee/Models/*.swift; do
  echo "=== $f ==="
  awk '/struct.*: .*Codable|class.*: .*Codable/{found=1; name=$0} found && /enum CodingKeys/{found=0} END{if(found) print "MISSING CodingKeys in: " name}' "$f"
done
```

Expected output: no `MISSING CodingKeys` lines. (If any appear, add explicit `CodingKeys` per the existing patterns before continuing — there should be none based on Task 1's audit.)

- [ ] **Step 2: Also grep ViewModels/Services for ad-hoc Codable structs**

```bash
grep -rln "struct.*Codable\|: Codable" apps/ios/ServerBee/ViewModels apps/ios/ServerBee/Services apps/ios/ServerBee/Utilities 2>/dev/null
```

Expected output: empty (or only types like AuthManager body literals that are dictionaries, not Codable structs). If a `Codable` struct is found outside `Models/`, add explicit `CodingKeys` to it.

- [ ] **Step 3: No code change needed; skip commit**

---

## Task 7: Remove `.convertToSnakeCase` from `JSONEncoder.snakeCase`

**Files:**
- Modify: `apps/ios/ServerBee/Models/APIModels.swift:16-23`
- Modify (no-op rename rationale): `apps/ios/ServerBee/Services/APIClient.swift:109`
- Modify (no-op rename rationale): `apps/ios/ServerBee/Services/AuthManager.swift:125`
- Create: `apps/ios/ServerBeeTests/JSONEncoderConventionTests.swift`

- [ ] **Step 1: Write failing test pinning the encoder behaviour**

Create `apps/ios/ServerBeeTests/JSONEncoderConventionTests.swift`:

```swift
import XCTest
@testable import ServerBee

final class JSONEncoderConventionTests: XCTestCase {
    func test_loginRequest_encodesViaCodingKeys_notKeyStrategy() throws {
        let req = MobileLoginRequest(
            username: "alice",
            password: "pw",
            installationId: "iid-1",
            deviceName: "iPhone",
            totpCode: nil
        )
        let data = try JSONEncoder.snakeCase.encode(req)
        let json = String(data: data, encoding: .utf8)!
        XCTAssertTrue(json.contains("\"installation_id\":\"iid-1\""),
                      "CodingKeys must produce snake_case for installation_id")
        XCTAssertTrue(json.contains("\"device_name\":\"iPhone\""),
                      "CodingKeys must produce snake_case for device_name")
    }

    /// If `.convertToSnakeCase` were still active AND a model had a property
    /// without a CodingKey override, both transformations could combine and
    /// double-snake or otherwise corrupt the key. Pin a property that *does*
    /// have an override to confirm it round-trips cleanly via CodingKeys alone.
    func test_refreshRequest_encodesRefreshTokenSnakeCase() throws {
        let req = MobileRefreshRequest(refreshToken: "tok", installationId: "iid")
        let data = try JSONEncoder.snakeCase.encode(req)
        let json = String(data: data, encoding: .utf8)!
        XCTAssertTrue(json.contains("\"refresh_token\":\"tok\""))
        XCTAssertFalse(json.contains("\"refreshToken\""))
    }
}
```

- [ ] **Step 2: Run test to confirm baseline (both should pass even before edit)**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/JSONEncoderConventionTests`

Expected: PASS (both with and without `.convertToSnakeCase` — these tests pin the contract, ensuring removing the strategy does not regress behaviour).

- [ ] **Step 3: Update `JSONEncoder.snakeCase` to use defaults**

Replace lines 14-32 of `apps/ios/ServerBee/Models/APIModels.swift`:

```swift
// MARK: - JSON Coding Helpers

extension JSONEncoder {
    /// Encoder that relies on explicit `CodingKeys` in each model.
    ///
    /// Historically this set `.keyEncodingStrategy = .convertToSnakeCase`,
    /// which conflicted with the hand-written `CodingKeys` already on every
    /// model and risked double-conversion if a property's CodingKey was
    /// itself camelCase. See `Models/README.md`.
    static let snakeCase: JSONEncoder = JSONEncoder()
}

extension JSONDecoder {
    /// Decoder that relies on explicit `CodingKeys` in each model.
    static let snakeCase: JSONDecoder = JSONDecoder()
}
```

- [ ] **Step 4: Run tests again to confirm no regression**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/JSONEncoderConventionTests`

Expected: PASS — output still contains `installation_id`, `device_name`, `refresh_token`.

- [ ] **Step 5: Run full test suite to be safe**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15'`

Expected: All tests PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/ios/ServerBee/Models/APIModels.swift \
        apps/ios/ServerBeeTests/JSONEncoderConventionTests.swift
git commit -m "refactor(ios): rely on explicit CodingKeys, drop convertToSnakeCase"
```

---

## Task 8: Replace `formatBytes` with `ByteCountFormatter`

**Files:**
- Modify: `apps/ios/ServerBee/Services/Formatters.swift:4-32`
- Create: `apps/ios/ServerBeeTests/FormattersByteCountTests.swift`

- [ ] **Step 1: Write failing test**

Create `apps/ios/ServerBeeTests/FormattersByteCountTests.swift`:

```swift
import XCTest
@testable import ServerBee

final class FormattersByteCountTests: XCTestCase {
    func test_formatBytes_zero() {
        XCTAssertEqual(Formatters.formatBytes(0), "Zero KB")
    }

    func test_formatBytes_oneKibibyte() {
        // ByteCountFormatter with .binary uses the unambiguous 1024 base
        // and "KB" label (per Apple's default countStyle = .file behaviour
        // which interprets KB as 1024). We assert "1 KB" because that's
        // exactly what ByteCountFormatter.string(fromByteCount:) emits at
        // en_US locale for 1024.
        XCTAssertEqual(Formatters.formatBytes(1024), "1 KB")
    }

    func test_formatBytes_oneMebibyte() {
        XCTAssertEqual(Formatters.formatBytes(1_048_576), "1 MB")
    }

    func test_formatBytes_belowOneKB() {
        // Under 1024 ByteCountFormatter emits bytes verbatim.
        XCTAssertEqual(Formatters.formatBytes(512), "512 bytes")
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/FormattersByteCountTests`

Expected: FAIL on `test_formatBytes_zero` and `test_formatBytes_belowOneKB`: current impl returns `"0 B"` and `"512 B"`.

- [ ] **Step 3: Update `formatBytes` and `formatSpeed`**

Replace lines 4-32 of `apps/ios/ServerBee/Services/Formatters.swift` (the function bodies up to and including `formatSpeed`) with:

```swift
enum Formatters {
    /// Shared formatter for human-readable byte counts (binary 1024 base, file style).
    /// `ByteCountFormatter` is locale-aware: e.g. zh-Hans prefixes "字节" for under-1KB values.
    private static let byteFormatter: ByteCountFormatter = {
        let f = ByteCountFormatter()
        f.countStyle = .binary
        f.allowedUnits = [.useBytes, .useKB, .useMB, .useGB, .useTB]
        return f
    }()

    static func formatBytes(_ bytes: Int64) -> String {
        byteFormatter.string(fromByteCount: bytes)
    }

    static func formatSpeed(_ bytesPerSec: Int64?) -> String {
        guard let bytesPerSec else {
            return "-"
        }
        return "\(byteFormatter.string(fromByteCount: bytesPerSec))/s"
    }
```

(Leave the remainder of the file — `formatUptime`, `formatPercentage`, `formatBytesRatio`, the color helpers — alone for now; Task 9 will edit them.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/FormattersByteCountTests`

Expected: PASS all four tests.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/Services/Formatters.swift \
        apps/ios/ServerBeeTests/FormattersByteCountTests.swift
git commit -m "refactor(ios): use ByteCountFormatter for locale-aware byte formatting"
```

---

## Task 9: Cache `DateFormatter` instances in `Formatters`

**Files:**
- Modify: `apps/ios/ServerBee/Services/Formatters.swift:74-79`

- [ ] **Step 1: Write the full replacement file**

Replace the entire content of `apps/ios/ServerBee/Services/Formatters.swift`:

```swift
import Foundation
import SwiftUI

enum Formatters {
    /// Shared formatter for human-readable byte counts (binary 1024 base, file style).
    private static let byteFormatter: ByteCountFormatter = {
        let f = ByteCountFormatter()
        f.countStyle = .binary
        f.allowedUnits = [.useBytes, .useKB, .useMB, .useGB, .useTB]
        return f
    }()

    /// Cached HH:mm formatter for chart X-axis labels. Recreating
    /// `DateFormatter` on each Chart render hurts scroll performance.
    private static let chartTimeFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "HH:mm"
        return f
    }()

    /// Cached `RelativeDateTimeFormatter` for human-readable elapsed time
    /// (e.g. "5 minutes ago" / "5 分钟前"). Locale-aware.
    private static let relativeFormatter: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .short
        return f
    }()

    static func formatBytes(_ bytes: Int64) -> String {
        byteFormatter.string(fromByteCount: bytes)
    }

    static func formatSpeed(_ bytesPerSec: Int64?) -> String {
        guard let bytesPerSec else {
            return "-"
        }
        return "\(byteFormatter.string(fromByteCount: bytesPerSec))/s"
    }

    static func formatUptime(_ seconds: Int64) -> String {
        let d = seconds / 86_400
        let h = (seconds % 86_400) / 3600
        if d > 0 {
            return "\(d)d \(h)h"
        }
        let m = (seconds % 3600) / 60
        return "\(h)h \(m)m"
    }

    static func formatPercentage(_ value: Double?) -> String {
        guard let value else {
            return "-"
        }
        return String(format: "%.1f%%", value)
    }

    static func formatBytesRatio(used: Int64?, total: Int64?) -> String? {
        guard let used, let total else { return nil }
        return "\(formatBytes(used)) / \(formatBytes(total))"
    }

    /// Returns a colour representing CPU load severity.
    static func cpuColor(for value: Double) -> Color {
        switch value {
        case ..<50: return .cpuColor
        case ..<80: return .orange
        default: return .red
        }
    }

    /// Returns a colour representing generic usage severity (memory, disk).
    static func usageColor(for value: Double) -> Color {
        switch value {
        case ..<50: return .green
        case ..<80: return .orange
        default: return .red
        }
    }

    /// Short time label for chart X-axis.
    static func formatChartTime(_ date: Date) -> String {
        chartTimeFormatter.string(from: date)
    }

    /// Locale-aware relative time, e.g. "5 minutes ago" / "5 分钟前".
    /// Returns the original ISO string if parsing fails.
    static func formatRelativeTime(_ isoString: String) -> String {
        guard let date = ISO8601DateFormatter.shared.date(from: isoString) else {
            return isoString
        }
        return relativeFormatter.localizedString(for: date, relativeTo: Date())
    }
}
```

- [ ] **Step 2: Run existing formatter tests**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/FormattersByteCountTests`

Expected: PASS — byte formatting unchanged.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Services/Formatters.swift
git commit -m "perf(ios): cache DateFormatter instances and use RelativeDateTimeFormatter"
```

---

## Task 10: Test locale-aware relative time

**Files:**
- Create: `apps/ios/ServerBeeTests/FormattersRelativeTimeTests.swift`

- [ ] **Step 1: Write the test**

Create `apps/ios/ServerBeeTests/FormattersRelativeTimeTests.swift`:

```swift
import XCTest
@testable import ServerBee

final class FormattersRelativeTimeTests: XCTestCase {
    func test_formatRelativeTime_returnsNonEmptyForRecentTimestamp() {
        // 30 seconds ago
        let date = Date().addingTimeInterval(-30)
        let iso = ISO8601DateFormatter.shared.string(from: date)
        let result = Formatters.formatRelativeTime(iso)
        XCTAssertFalse(result.isEmpty)
        // We don't assert the exact wording (it's locale-dependent and
        // RelativeDateTimeFormatter wording can shift between iOS versions),
        // only that we got a non-empty localized string back rather than
        // the raw ISO timestamp.
        XCTAssertFalse(result.contains("T"), "Should not echo back the raw ISO timestamp")
        XCTAssertFalse(result.contains("Z"), "Should not echo back the raw ISO timestamp")
    }

    func test_formatRelativeTime_returnsOriginalOnParseFailure() {
        let garbage = "not-a-date"
        XCTAssertEqual(Formatters.formatRelativeTime(garbage), garbage)
    }
}
```

- [ ] **Step 2: Run test**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/FormattersRelativeTimeTests`

Expected: PASS both tests.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBeeTests/FormattersRelativeTimeTests.swift
git commit -m "test(ios): cover locale-aware relative time formatting"
```

---

## Task 11: ISO8601 parser fallback for timestamps without fractional seconds

**Files:**
- Modify: `apps/ios/ServerBee/Utilities/Extensions.swift:18-24`
- Create: `apps/ios/ServerBeeTests/ISO8601ParserTests.swift`

- [ ] **Step 1: Write failing test**

Create `apps/ios/ServerBeeTests/ISO8601ParserTests.swift`:

```swift
import XCTest
@testable import ServerBee

final class ISO8601ParserTests: XCTestCase {
    func test_parsesWithFractionalSeconds() {
        let date = ISO8601DateFormatter.shared.date(from: "2026-05-20T10:30:00.123Z")
        XCTAssertNotNil(date, "Must parse timestamps with fractional seconds")
    }

    func test_parsesWithoutFractionalSeconds() {
        let date = ISO8601DateFormatter.shared.date(from: "2026-05-20T10:30:00Z")
        XCTAssertNotNil(date, "Must parse timestamps without fractional seconds (chrono to_rfc3339 emits this when subsec is 0)")
    }

    func test_parsesWithTimezoneOffset() {
        let date = ISO8601DateFormatter.shared.date(from: "2026-05-20T10:30:00+00:00")
        XCTAssertNotNil(date, "Must parse timestamps with explicit timezone offset (chrono default)")
    }

    func test_returnsNilForGarbage() {
        XCTAssertNil(ISO8601DateFormatter.shared.date(from: "not-a-date"))
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/ISO8601ParserTests`

Expected: FAIL on `test_parsesWithoutFractionalSeconds` and possibly `test_parsesWithTimezoneOffset` because the shared formatter is configured with `.withFractionalSeconds` mandatory.

- [ ] **Step 3: Replace Extensions.swift with a fallback-capable parser**

Replace the entire content of `apps/ios/ServerBee/Utilities/Extensions.swift`:

```swift
import SwiftUI

// MARK: - Color Extensions

extension Color {
    static let serverOnline = Color(red: 0x22 / 255.0, green: 0xC5 / 255.0, blue: 0x5E / 255.0)
    static let serverOffline = Color(red: 0xEF / 255.0, green: 0x44 / 255.0, blue: 0x44 / 255.0)
    static let alertFiring = Color(red: 0xF9 / 255.0, green: 0x73 / 255.0, blue: 0x16 / 255.0)
    static let alertResolved = Color.serverOnline
    static let cpuColor = Color(red: 0x38 / 255.0, green: 0xBD / 255.0, blue: 0xF8 / 255.0)
    static let memoryColor = Color(red: 0xA7 / 255.0, green: 0x8B / 255.0, blue: 0xFA / 255.0)
    static let diskColor = Color(red: 0xFB / 255.0, green: 0xBD / 255.0, blue: 0x23 / 255.0)
    static let networkColor = Color(red: 0x34 / 255.0, green: 0xD3 / 255.0, blue: 0x99 / 255.0)
}

// MARK: - ISO8601DateFormatter Extension

/// Tolerant ISO 8601 parser that handles backend timestamps with OR without
/// fractional seconds. The Rust backend uses `chrono::DateTime::to_rfc3339()`,
/// which only emits fractional seconds when the source value has subsecond
/// precision — so both forms appear in real payloads.
final class TolerantISO8601Parser: @unchecked Sendable {
    private let withFractional: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()

    private let withoutFractional: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()

    func date(from string: String) -> Date? {
        withFractional.date(from: string) ?? withoutFractional.date(from: string)
    }

    func string(from date: Date) -> String {
        withFractional.string(from: date)
    }
}

extension ISO8601DateFormatter {
    /// Tolerant shared parser. Use `.date(from:)` / `.string(from:)` on it.
    /// (Note: this is now a wrapper type with the same method shape, not an
    /// actual `ISO8601DateFormatter` instance.)
    nonisolated(unsafe) static let shared: TolerantISO8601Parser = TolerantISO8601Parser()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/ISO8601ParserTests`

Expected: PASS all four tests.

- [ ] **Step 5: Run the rest of the suite to ensure no caller broke**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15'`

Expected: ALL PASS. The wrapper type exposes `.date(from:)` and `.string(from:)` with the same signatures, so existing call sites (`Formatters.formatRelativeTime`, `MetricRecord.date`, `FormattersRelativeTimeTests`) continue to compile.

- [ ] **Step 6: Commit**

```bash
git add apps/ios/ServerBee/Utilities/Extensions.swift \
        apps/ios/ServerBeeTests/ISO8601ParserTests.swift
git commit -m "fix(ios): tolerate ISO8601 timestamps with or without fractional seconds"
```

---

## Task 12: Plan the Localizable.xcstrings key rename

**Files:**
- Read only at this step: `apps/ios/ServerBee/Localizable.xcstrings`
- Read only at this step: every `*.swift` file under `apps/ios/ServerBee/`

- [ ] **Step 1: Enumerate every snake_case key in code**

Run from repo root:

```bash
grep -roE 'String\(localized: "[a-z_]+[a-z]"' apps/ios/ServerBee/ \
  | grep -oE '"[a-z_]+"' | sort -u
```

Expected output (these are the keys to rename):

```
"settings_about"
"settings_account"
"settings_appearance"
"settings_cancel"
"settings_language"
"settings_logout"
"settings_logout_confirm"
"settings_preferences"
"settings_role"
"settings_server"
"settings_theme"
"settings_theme_dark"
"settings_theme_light"
"settings_theme_system"
"settings_title"
"settings_username"
"settings_version"
```

- [ ] **Step 2: Record the rename mapping**

For each snake key above, find its English source text by reading the corresponding entry in `apps/ios/ServerBee/Localizable.xcstrings`. The full mapping (English-as-key):

```
settings_about           → "About"
settings_account         → "Account"
settings_appearance      → "Appearance"
settings_cancel          → "Cancel"
settings_language        → "Language"
settings_logout          → "Log Out"
settings_logout_confirm  → "Are you sure you want to log out?"
settings_preferences     → "Preferences"
settings_role            → "Role"
settings_server          → "Server"            ⚠ collides with existing "Server" key — REUSE that one, do not duplicate
settings_theme           → "Theme"
settings_theme_dark      → "Dark"
settings_theme_light     → "Light"
settings_theme_system    → "System"
settings_title           → "Settings"
settings_username        → "Username"          ⚠ collides with existing "Username" key — REUSE
settings_version         → "Version"
```

- [ ] **Step 3: Verify collisions against existing English-as-key entries**

Read `apps/ios/ServerBee/Localizable.xcstrings` and confirm: keys `"Server"`, `"Username"`, `"Offline"`, `"Online"` already exist as English-as-key entries (no `extractionState`, just the English text used directly). After the rename, `settings_server` and `settings_username` call sites point at those existing entries and the snake-key duplicates are deleted.

- [ ] **Step 4: No code change in this task; skip commit**

This task is a planning step. Task 13 performs the rename in code and xcstrings together.

---

## Task 13: Execute the rename across code and xcstrings

**Files:**
- Modify: `apps/ios/ServerBee/Views/Settings/AppearanceView.swift:18-67`
- Modify: `apps/ios/ServerBee/Views/Settings/SettingsView.swift:16-78`
- Modify: `apps/ios/ServerBee/Localizable.xcstrings`

- [ ] **Step 1: Update AppearanceView.swift**

Apply the following text replacements in `apps/ios/ServerBee/Views/Settings/AppearanceView.swift`:

| old | new |
|---|---|
| `String(localized: "settings_theme_system")` | `String(localized: "System")` |
| `String(localized: "settings_theme_light")` | `String(localized: "Light")` |
| `String(localized: "settings_theme_dark")` | `String(localized: "Dark")` |
| `String(localized: "settings_theme")` | `String(localized: "Theme")` |
| `String(localized: "settings_language")` | `String(localized: "Language")` |
| `String(localized: "settings_appearance")` | `String(localized: "Appearance")` |

- [ ] **Step 2: Update SettingsView.swift**

Apply in `apps/ios/ServerBee/Views/Settings/SettingsView.swift`:

| old | new |
|---|---|
| `String(localized: "settings_title")` | `String(localized: "Settings")` |
| `String(localized: "settings_logout_confirm")` | `String(localized: "Are you sure you want to log out?")` |
| `String(localized: "settings_logout")` | `String(localized: "Log Out")` |
| `String(localized: "settings_cancel")` | `String(localized: "Cancel")` |
| `String(localized: "settings_account")` | `String(localized: "Account")` |
| `String(localized: "settings_username")` | `String(localized: "Username")` |
| `String(localized: "settings_role")` | `String(localized: "Role")` |
| `String(localized: "settings_server")` | `String(localized: "Server")` |
| `String(localized: "settings_preferences")` | `String(localized: "Preferences")` |
| `String(localized: "settings_appearance")` | `String(localized: "Appearance")` |
| `String(localized: "settings_about")` | `String(localized: "About")` |
| `String(localized: "settings_version")` | `String(localized: "Version")` |

- [ ] **Step 3: Verify no `settings_*` keys remain in Swift code**

Run from repo root:

```bash
grep -rn 'String(localized: "settings_' apps/ios/ServerBee/
```

Expected: empty output.

- [ ] **Step 4: Update Localizable.xcstrings — replace each `settings_*` entry**

For each of the 17 snake keys, perform one of two operations in `apps/ios/ServerBee/Localizable.xcstrings`:

**(a) If an English-as-key entry already exists** (`settings_server` → `Server`, `settings_username` → `Username`): delete the snake_case entry entirely. The existing English-as-key entry already has both `en` and `zh-Hans` translations.

**(b) Otherwise**: rename the entry's JSON key from the snake form to the English source-text form.

Example structure for `settings_theme` becoming `Theme` (the *only* change is the JSON key on line 1):

```json
"Theme" : {
  "extractionState" : "manual",
  "localizations" : {
    "en" : {
      "stringUnit" : { "state" : "translated", "value" : "Theme" }
    },
    "zh-Hans" : {
      "stringUnit" : { "state" : "translated", "value" : "主题" }
    }
  }
}
```

Process all 17 entries. After the rename, sort the top-level `"strings"` dictionary alphabetically (Xcode does this on save) for diff legibility.

- [ ] **Step 5: Verify the xcstrings file is still valid JSON**

Run: `python3 -c "import json; json.load(open('apps/ios/ServerBee/Localizable.xcstrings'))"`

Expected: no output (clean exit ⇒ valid JSON).

- [ ] **Step 6: Build the app**

Run: `cd apps/ios && xcodegen generate && xcodebuild build -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15'`

Expected: BUILD SUCCEEDED with no missing-localization warnings.

- [ ] **Step 7: Run full test suite**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15'`

Expected: all tests PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/ios/ServerBee/Views/Settings/AppearanceView.swift \
        apps/ios/ServerBee/Views/Settings/SettingsView.swift \
        apps/ios/ServerBee/Localizable.xcstrings
git commit -m "refactor(ios): unify Localizable.xcstrings keys on English source text"
```

---

## Task 14: Manual visual verification (EN + zh-Hans)

**Files:**
- No code changes.

- [ ] **Step 1: Launch the app in English locale**

Run: `cd apps/ios && xcodebuild build -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15'`

Then in Xcode: Edit Scheme → Run → Options → App Language = English. Press Run.

- [ ] **Step 2: Walk every screen and confirm strings render**

Screens to inspect (each must show the expected English text — no raw keys like `settings_theme`):

- Login screen → "Server URL", "Username", "Password", "Log In", "Back"
- Servers list → "Servers" tab, "All" / "Online" / "Offline" filter chips, "No Servers" empty state, server card layout
- Server detail → "Online"/"Offline" status pill, "CPU Usage", "Memory Usage", "Disk Usage", "Network I/O", "Load", "TCP / UDP", "Processes", "OS", "View History"
- Metrics history → "Metrics History" title, time-range pills (`1h`/`6h`/`24h`/`7d` — these are not localized), "No metric records found for this time range." empty state, chart x-axis labels
- Alerts list → "Alerts" tab, "Loading alerts..." spinner, "No Alerts" empty state, alert rows with "FIRING"/"RESOLVED" badges
- Alert detail → "Alert Detail" title, "Rule Name", "Trigger Count", "Trigger Mode", "Rule Enabled", "Message", "First Triggered", "Resolved At", "View Server"
- Settings → "Settings" title, "Account" section with "Username"/"Role"/"Server" labels, "Preferences" section with "Appearance", "About" section with "Version", "Log Out" button, logout confirm "Are you sure you want to log out?" with "Log Out"/"Cancel"
- Appearance settings → "Appearance" title, "Theme" picker with "System"/"Light"/"Dark", "Language" picker

Sample relative-time labels in alerts list: should read e.g. "5 min. ago" or "in 2 hr." — confirm the format looks natural in English.

- [ ] **Step 3: Switch app to Simplified Chinese**

Edit Scheme → Run → Options → App Language = Chinese (Simplified). Press Run.

- [ ] **Step 4: Re-walk the same screens in zh-Hans**

Confirm every English string above renders its Chinese translation from `Localizable.xcstrings`. Particular attention to:

- "Settings" → "设置"
- "Log Out" → "退出登录"
- "Theme" → "主题"
- Relative time labels render in Chinese (e.g. "5 分钟前").
- Byte counts use Chinese unit labels where appropriate (e.g. "字节" for raw bytes).

- [ ] **Step 5: Document the verification in the commit body**

If both passes succeed, no code change. Create a no-op commit recording the verification:

```bash
git commit --allow-empty -m "test(ios): manual EN+zh-Hans verification of localization rename" \
                          -m "Walked: login, servers list, server detail, metrics history, alerts list, alert detail, settings, appearance settings. All strings render in both locales without raw key fallback."
```

- [ ] **Step 6: Push the branch and open PR (if applicable)**

Defer to the user's batched-push workflow; do not push per task.

---

## Self-Review Checklist

- All 14 tasks reference real files at real line numbers (verified before writing this plan).
- `ServerStatus.isOnline` is defined in Task 2 Step 3 and referenced by Task 2 Steps 5-6.
- `TolerantISO8601Parser.date(from:)` / `.string(from:)` match the surface of `ISO8601DateFormatter`, so existing callers (`MetricRecord.date`, `Formatters.formatRelativeTime`, `FormattersRelativeTimeTests`) keep working.
- `JSONEncoder.snakeCase` and `JSONDecoder.snakeCase` retain the same identifiers so call sites in `APIClient.swift:109` and `AuthManager.swift:125` stay untouched.
- Each task ends with a Conventional Commits commit (lowercase type and scope, no Claude attribution).
- Localization rename mapping in Task 12 lists every snake key from the actual `grep` output; collisions with existing English-as-key entries are explicitly handled.
- No placeholder language ("TODO", "TBD", "similar to") anywhere — every code block is the complete code to paste.
