# iOS Plan 6: UI Polish and Accessibility

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the iOS UI from "works on my device" to a level appropriate for App Store submission: full Dark Mode color fidelity via Asset Catalog, Dynamic Type compatibility, VoiceOver accessibility labels, honest error states with retry, and debounced list filtering.

**Architecture:** Color literals migrate to Asset Catalog so light/dark variants are first-class. `@ScaledMetric` and semantic SwiftUI fonts replace fixed sizes. Each card view becomes a single accessibility element with composed label and value. ViewModels expose an `errorMessage` state; List views render dedicated error states with retry buttons. Search input is debounced via `.task(id:)` with 250ms sleep.

**Tech Stack:** SwiftUI, Asset Catalog, `@ScaledMetric`, `RelativeDateTimeFormatter`, VoiceOver, XCTest.

**Depends on:** Plan 1 (`ServerBeeTests` target).

---

## Task 1: Create Asset Catalog color sets for status & metric colors

**Files:**
- Create: `apps/ios/ServerBee/Assets.xcassets/Colors/Contents.json`
- Create: `apps/ios/ServerBee/Assets.xcassets/Colors/ServerOnline.colorset/Contents.json`
- Create: `apps/ios/ServerBee/Assets.xcassets/Colors/ServerOffline.colorset/Contents.json`
- Create: `apps/ios/ServerBee/Assets.xcassets/Colors/WarningAmber.colorset/Contents.json`
- Create: `apps/ios/ServerBee/Assets.xcassets/Colors/BrandAccent.colorset/Contents.json`
- Create: `apps/ios/ServerBee/Assets.xcassets/Colors/CPUColor.colorset/Contents.json`
- Create: `apps/ios/ServerBee/Assets.xcassets/Colors/MemoryColor.colorset/Contents.json`
- Create: `apps/ios/ServerBee/Assets.xcassets/Colors/DiskColor.colorset/Contents.json`
- Create: `apps/ios/ServerBee/Assets.xcassets/Colors/NetworkColor.colorset/Contents.json`
- Create: `apps/ios/ServerBee/Assets.xcassets/Colors/AlertFiring.colorset/Contents.json`

- [ ] **Step 1: Create the folder Contents.json**

Write `apps/ios/ServerBee/Assets.xcassets/Colors/Contents.json`:

```json
{
  "info" : {
    "author" : "xcode",
    "version" : 1
  },
  "properties" : {
    "provides-namespace" : false
  }
}
```

- [ ] **Step 2: Write `ServerOnline.colorset/Contents.json` (Light #22C55E / Dark #34D399)**

```json
{
  "colors" : [
    {
      "color" : {
        "color-space" : "srgb",
        "components" : {
          "alpha" : "1.000",
          "blue" : "0x5E",
          "green" : "0xC5",
          "red" : "0x22"
        }
      },
      "idiom" : "universal"
    },
    {
      "appearances" : [
        {
          "appearance" : "luminosity",
          "value" : "dark"
        }
      ],
      "color" : {
        "color-space" : "srgb",
        "components" : {
          "alpha" : "1.000",
          "blue" : "0x99",
          "green" : "0xD3",
          "red" : "0x34"
        }
      },
      "idiom" : "universal"
    }
  ],
  "info" : {
    "author" : "xcode",
    "version" : 1
  }
}
```

- [ ] **Step 3: Create the remaining 8 colorsets following the same shape**

Follow the same JSON structure as Step 2, using each name's hex pair below for the light entry first, dark entry second. Replace `red` / `green` / `blue` byte values accordingly.

- `ServerOffline.colorset` — Light `#EF4444` (R=0xEF G=0x44 B=0x44) / Dark `#F87171` (R=0xF8 G=0x71 B=0x71)
- `WarningAmber.colorset` — Light `#F59E0B` (R=0xF5 G=0x9E B=0x0B) / Dark `#FBBF24` (R=0xFB G=0xBF B=0x24)
- `BrandAccent.colorset` — Light `#22C55E` (R=0x22 G=0xC5 B=0x5E) / Dark `#34D399` (R=0x34 G=0xD3 B=0x99) — matches the existing `AccentColor`
- `CPUColor.colorset` — Light `#38BDF8` (R=0x38 G=0xBD B=0xF8) / Dark `#7DD3FC` (R=0x7D G=0xD3 B=0xFC)
- `MemoryColor.colorset` — Light `#A78BFA` (R=0xA7 G=0x8B B=0xFA) / Dark `#C4B5FD` (R=0xC4 G=0xB5 B=0xFD)
- `DiskColor.colorset` — Light `#FBBD23` (R=0xFB G=0xBD B=0x23) / Dark `#FCD34D` (R=0xFC G=0xD3 B=0x4D)
- `NetworkColor.colorset` — Light `#34D399` (R=0x34 G=0xD3 B=0x99) / Dark `#6EE7B7` (R=0x6E G=0xE7 B=0xB7)
- `AlertFiring.colorset` — Light `#F97316` (R=0xF9 G=0x73 B=0x16) / Dark `#FB923C` (R=0xFB G=0x92 B=0x3C)

- [ ] **Step 4: Commit**

```bash
git add apps/ios/ServerBee/Assets.xcassets/Colors
git commit -m "feat(ios): add asset catalog color sets with dark mode variants"
```

---

## Task 2: Migrate `Color` extensions to Asset Catalog references

**Files:**
- Modify: `apps/ios/ServerBee/Utilities/Extensions.swift:5-13`

- [ ] **Step 1: Replace literal `Color(red:...)` definitions with named-asset references**

Overwrite `apps/ios/ServerBee/Utilities/Extensions.swift`:

```swift
import SwiftUI

// MARK: - Color Extensions

extension Color {
    static let serverOnline = Color("ServerOnline")
    static let serverOffline = Color("ServerOffline")
    static let alertFiring = Color("AlertFiring")
    static let alertResolved = Color("ServerOnline")
    static let warningAmber = Color("WarningAmber")
    static let brandAccent = Color("BrandAccent")
    static let cpuColor = Color("CPUColor")
    static let memoryColor = Color("MemoryColor")
    static let diskColor = Color("DiskColor")
    static let networkColor = Color("NetworkColor")
}

// MARK: - ISO8601DateFormatter Extension

extension ISO8601DateFormatter {
    nonisolated(unsafe) static let shared: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()
}
```

- [ ] **Step 2: Build & verify in simulator (light / dark)**

Run:

```bash
xcodebuild -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16' build
```

Expected: BUILD SUCCEEDED. Launch in simulator, toggle Dark Mode (Features → Toggle Appearance, ⌘⇧A), verify green status dot reads correctly in both modes.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Utilities/Extensions.swift
git commit -m "refactor(ios): point Color extensions at asset catalog entries"
```

---

## Task 3: Switch `AppearanceView` theme Picker to default menu style

**Files:**
- Modify: `apps/ios/ServerBee/Views/Settings/AppearanceView.swift:45-69`

- [ ] **Step 1: Replace the `body` block**

Overwrite the `body` property in `apps/ios/ServerBee/Views/Settings/AppearanceView.swift`:

```swift
    var body: some View {
        List {
            Section {
                Picker(selection: $theme) {
                    ForEach(AppTheme.allCases, id: \.rawValue) { option in
                        Text(option.localizedName).tag(option.rawValue)
                    }
                } label: {
                    Text(String(localized: "settings_theme"))
                }
            } header: {
                Text(String(localized: "settings_theme"))
            }

            Section {
                Picker(selection: $locale) {
                    ForEach(AppLanguage.allCases, id: \.rawValue) { option in
                        Text(option.displayName).tag(option.rawValue)
                    }
                } label: {
                    Text(String(localized: "settings_language"))
                }
            } header: {
                Text(String(localized: "settings_language"))
            }
        }
        .navigationTitle(String(localized: "settings_appearance"))
        .preferredColorScheme(selectedTheme.colorScheme)
    }
```

Note: The default Picker style inside a `List` row renders as a menu / chevron, so the section header is no longer duplicated by an inline title.

- [ ] **Step 2: Build & verify**

```bash
xcodebuild -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16' build
```

Open Settings → Appearance: section header appears once, picker shows current selection on the right; tapping reveals a menu.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Views/Settings/AppearanceView.swift
git commit -m "fix(ios): use default Picker style in AppearanceView to avoid duplicated section header"
```

---

## Task 4: Make `MetricCardView` respect Dynamic Type

**Files:**
- Modify: `apps/ios/ServerBee/Views/Servers/MetricCardView.swift:5-31`

- [ ] **Step 1: Replace the view body**

Overwrite `apps/ios/ServerBee/Views/Servers/MetricCardView.swift`:

```swift
import SwiftUI

/// A compact card displaying a single metric with label, value, and optional subtitle.
/// Used in the 2-column metrics grid on the server detail view.
struct MetricCardView: View {
    let label: String
    let value: String
    var subtitle: String?
    var valueColor: Color = .primary

    @ScaledMetric(relativeTo: .body) private var verticalPad: CGFloat = 14
    @ScaledMetric(relativeTo: .body) private var horizontalPad: CGFloat = 14

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.title2.bold())
                .foregroundStyle(valueColor)
                .minimumScaleFactor(0.7)
                .lineLimit(1)
            if let subtitle {
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, horizontalPad)
        .padding(.vertical, verticalPad)
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(label))
        .accessibilityValue(Text(subtitle.map { "\(value), \($0)" } ?? value))
    }
}

#Preview {
    VStack(spacing: 12) {
        MetricCardView(
            label: "CPU",
            value: "45.2%",
            subtitle: "Intel i7-12700K",
            valueColor: .green
        )
        MetricCardView(
            label: "Memory",
            value: "72.3%",
            subtitle: "11.6 GB / 16.0 GB",
            valueColor: .orange
        )
    }
    .padding()
    .background(Color(.systemGroupedBackground))
}
```

- [ ] **Step 2: Commit**

```bash
git add apps/ios/ServerBee/Views/Servers/MetricCardView.swift
git commit -m "feat(ios): make MetricCardView dynamic-type and a11y friendly"
```

---

## Task 5: Make `ServerCardView` Dynamic-Type-friendly and accessible

**Files:**
- Modify: `apps/ios/ServerBee/Views/Servers/ServerCardView.swift:5-90`

- [ ] **Step 1: Replace the file**

Overwrite `apps/ios/ServerBee/Views/Servers/ServerCardView.swift`:

```swift
import SwiftUI

/// A card representing a single server in the servers list.
/// Shows online status, name, IP, and key metric pills (CPU, Memory, OS).
struct ServerCardView: View {
    let server: ServerStatus

    @ScaledMetric(relativeTo: .body) private var cardPad: CGFloat = 14
    @ScaledMetric(relativeTo: .caption2) private var dotSize: CGFloat = 10

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Circle()
                    .fill(server.online ? Color.serverOnline : Color.serverOffline)
                    .frame(width: dotSize, height: dotSize)
                    .accessibilityHidden(true)

                Text(server.name)
                    .font(.headline)
                    .lineLimit(1)

                Spacer()

                if let lastActive = server.lastActiveAt, !server.online {
                    Text(Formatters.formatRelativeTime(lastActive))
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            if let ip = server.primaryIP {
                Text(ip)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            HStack(spacing: 8) {
                MetricPill(
                    label: String(localized: "CPU"),
                    value: Formatters.formatPercentage(server.cpuUsage),
                    color: .cpuColor
                )

                MetricPill(
                    label: String(localized: "MEM"),
                    value: server.memoryUsed.map { Formatters.formatBytes($0) } ?? "-",
                    color: .memoryColor
                )

                if let os = server.os {
                    MetricPill(
                        label: String(localized: "OS"),
                        value: os,
                        color: .secondary
                    )
                }
            }
        }
        .padding(cardPad)
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.05), radius: 3, y: 2)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(accessibilityLabelText))
    }

    private var accessibilityLabelText: String {
        let status = server.online
            ? String(localized: "Online")
            : String(localized: "Offline")
        let cpu = Formatters.formatPercentage(server.cpuUsage)
        let mem = server.memoryUsed.map { Formatters.formatBytes($0) } ?? "-"
        return String(
            format: String(localized: "%1$@, %2$@, CPU %3$@, memory %4$@"),
            server.name, status, cpu, mem
        )
    }
}

// MARK: - Metric Pill

/// A small pill showing a label and value, used at the bottom of the server card.
private struct MetricPill: View {
    let label: String
    let value: String
    let color: Color

    var body: some View {
        HStack(spacing: 4) {
            Text(label)
                .font(.caption2.bold())
                .foregroundStyle(color)
            Text(value)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(color.opacity(0.1))
        .clipShape(Capsule())
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(label))
        .accessibilityValue(Text(value))
    }
}

#Preview {
    ServerCardView(
        server: ServerStatus(
            id: "1",
            name: "Production Web Server",
            online: true,
            cpuUsage: 45.2,
            memoryTotal: 17_179_869_184,
            memoryUsed: 12_516_925_440,
            os: "Ubuntu 22.04",
            ipv4: "192.168.1.100"
        )
    )
    .padding()
    .background(Color(.systemGroupedBackground))
}
```

- [ ] **Step 2: Build**

```bash
xcodebuild -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16' build
```

Expected: BUILD SUCCEEDED.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Views/Servers/ServerCardView.swift
git commit -m "feat(ios): make ServerCardView dynamic-type and a11y friendly"
```

---

## Task 6: Make `ServerDetailView` header & badges Dynamic-Type-friendly

**Files:**
- Modify: `apps/ios/ServerBee/Views/Servers/ServerDetailView.swift:77-92`

- [ ] **Step 1: Replace the `statusBadge` computed property**

In `apps/ios/ServerBee/Views/Servers/ServerDetailView.swift`, replace the existing `statusBadge` definition with:

```swift
    private var statusBadge: some View {
        let label = server.online
            ? String(localized: "Online")
            : String(localized: "Offline")
        return HStack(spacing: 6) {
            Circle()
                .fill(server.online ? Color.serverOnline : Color.serverOffline)
                .frame(width: 10, height: 10)
                .accessibilityHidden(true)
            Text(label)
                .font(.subheadline.bold())
                .foregroundStyle(server.online ? Color.serverOnline : Color.serverOffline)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 5)
        .background(
            (server.online ? Color.serverOnline : Color.serverOffline).opacity(0.1)
        )
        .clipShape(Capsule())
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(String(localized: "Status")))
        .accessibilityValue(Text(label))
    }
```

- [ ] **Step 2: Build**

```bash
xcodebuild -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16' build
```

Expected: BUILD SUCCEEDED.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Views/Servers/ServerDetailView.swift
git commit -m "feat(ios): combine status badge into single accessibility element"
```

---

## Task 7: Audit decorative `Image(systemName:)` calls for `.accessibilityHidden(true)`

**Files:**
- Modify (only if used decoratively, i.e. paired with adjacent text):
  - `apps/ios/ServerBee/Views/Servers/ServerDetailView.swift` (`historyButton` chevron)
  - `apps/ios/ServerBee/Views/Servers/ServersListView.swift:93` (`noMatchesView` magnifying glass — also paired with text)

- [ ] **Step 1: Hide the chevron in `historyButton`**

In `apps/ios/ServerBee/Views/Servers/ServerDetailView.swift`, update the chevron Image inside `historyButton` so the row reads as a single element:

```swift
    private var historyButton: some View {
        NavigationLink {
            MetricsHistoryView(serverId: server.id)
        } label: {
            HStack {
                Label(String(localized: "View History"), systemImage: "chart.xyaxis.line")
                Spacer()
                Image(systemName: "chevron.right")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
            }
            .padding()
            .background(Color(.systemBackground))
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
            .accessibilityElement(children: .combine)
            .accessibilityLabel(Text(String(localized: "View History")))
            .accessibilityAddTraits(.isButton)
        }
        .buttonStyle(.plain)
    }
```

- [ ] **Step 2: Hide the magnifying glass in `noMatchesView`**

In `apps/ios/ServerBee/Views/Servers/ServersListView.swift`, replace the `noMatchesView` body so the icon is decorative:

```swift
    private var noMatchesView: some View {
        VStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .font(.largeTitle)
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            Text(String(localized: "No matching servers"))
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 60)
        .accessibilityElement(children: .combine)
    }
```

- [ ] **Step 3: Verify other `Image(systemName:` callers**

Run:

```bash
grep -RIn "Image(systemName:" apps/ios/ServerBee/Views | grep -v "accessibilityHidden\|Label("
```

Expected output: empty, or only matches inside `Label(...)` (which already combines text with the symbol for accessibility automatically). Any remaining bare `Image(systemName:)` calls without an accompanying text label must be reviewed in this step — if found, add `.accessibilityLabel("...")`.

- [ ] **Step 4: Commit**

```bash
git add apps/ios/ServerBee/Views/Servers/ServerDetailView.swift apps/ios/ServerBee/Views/Servers/ServersListView.swift
git commit -m "fix(ios): hide decorative SF Symbols from VoiceOver"
```

---

## Task 8: Make `AlertStatusBadge` a single accessibility element

**Files:**
- Modify: `apps/ios/ServerBee/Views/Alerts/AlertStatusBadge.swift:1-18`

- [ ] **Step 1: Replace the whole file**

Overwrite `apps/ios/ServerBee/Views/Alerts/AlertStatusBadge.swift`:

```swift
import SwiftUI

struct AlertStatusBadge: View {
    let status: AlertStatus
    var font: Font = .caption2.bold()
    var horizontalPadding: CGFloat = 8
    var verticalPadding: CGFloat = 3

    private var label: String {
        status == .firing
            ? String(localized: "FIRING")
            : String(localized: "RESOLVED")
    }

    var body: some View {
        Text(label)
            .font(font)
            .padding(.horizontal, horizontalPadding)
            .padding(.vertical, verticalPadding)
            .background(status == .firing ? Color.alertFiring : Color.alertResolved)
            .foregroundStyle(.white)
            .clipShape(Capsule())
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(Text(String(localized: "Alert status")))
            .accessibilityValue(Text(label))
    }
}
```

Note: switched from raw `.red` / `.green` to the asset-catalog–backed `.alertFiring` / `.alertResolved` so dark mode is consistent.

- [ ] **Step 2: Build**

```bash
xcodebuild -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16' build
```

Expected: BUILD SUCCEEDED.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Views/Alerts/AlertStatusBadge.swift
git commit -m "feat(ios): expose AlertStatusBadge as single VoiceOver element"
```

---

## Task 9: Make `AlertEventCardView` a single accessibility element

**Files:**
- Modify: `apps/ios/ServerBee/Views/Alerts/AlertEventCardView.swift:1-46`

- [ ] **Step 1: Replace the file**

Overwrite `apps/ios/ServerBee/Views/Alerts/AlertEventCardView.swift`:

```swift
import SwiftUI

struct AlertEventCardView: View {
    let event: MobileAlertEvent

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                AlertStatusBadge(status: event.status)

                Spacer()

                Text(Formatters.formatRelativeTime(event.updatedAt))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Text(event.ruleName)
                .font(.subheadline.bold())

            Text(event.serverName)
                .font(.caption)
                .foregroundStyle(.secondary)

            if !event.message.isEmpty {
                Text(event.message)
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .lineLimit(2)
            }

            if event.triggerCount > 1 {
                HStack {
                    Spacer()
                    Text("\u{00D7}\(event.triggerCount)")
                        .font(.caption2)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Color(.systemGray5))
                        .clipShape(Capsule())
                }
            }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(accessibilityLabelText))
    }

    private var accessibilityLabelText: String {
        let status = event.status == .firing
            ? String(localized: "Firing")
            : String(localized: "Resolved")
        let relative = Formatters.formatRelativeTime(event.updatedAt)
        var parts = [status, event.ruleName, event.serverName, relative]
        if event.triggerCount > 1 {
            parts.append(String(format: String(localized: "Triggered %d times"), event.triggerCount))
        }
        return parts.joined(separator: ", ")
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add apps/ios/ServerBee/Views/Alerts/AlertEventCardView.swift
git commit -m "feat(ios): combine AlertEventCardView into one accessibility element"
```

---

## Task 10: Manual VoiceOver smoke test

**Files:** none (verification only).

- [ ] **Step 1: Launch the app in the simulator**

```bash
xcodebuild -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16' build
open -a Simulator
```

- [ ] **Step 2: Enable VoiceOver and verify announcements**

In the simulator: Settings → Accessibility → VoiceOver → On. Then in the app:

- Servers tab: swipe through cards. Each card should announce one sentence containing name, online/offline, CPU %, memory.
- Tap into a server detail. Each metric card should announce label + value (e.g. "CPU, 45.2 percent"). The chevron-only "View History" row should announce only "View History, Button".
- Alerts tab: each event row should announce status + rule + server + relative time.
- Settings → Appearance: Theme picker reads as "Theme, System, Button" (or current value).

Document any mismatches and either patch in a follow-up step or open a ticket. Disable VoiceOver before continuing.

- [ ] **Step 3: Commit (only if any tweaks were needed)**

```bash
git add apps/ios/ServerBee/Views
git commit -m "fix(ios): tweak VoiceOver labels after manual audit"
```

If nothing changed, skip the commit.

---

## Task 11: Add `errorMessage` state to `ServersViewModel`

**Files:**
- Modify: `apps/ios/ServerBee/ViewModels/ServersViewModel.swift:19-68`
- Create: `apps/ios/ServerBeeTests/ViewModels/ServersViewModelErrorTests.swift`

- [ ] **Step 1: Write the failing test**

Create `apps/ios/ServerBeeTests/ViewModels/ServersViewModelErrorTests.swift`:

```swift
import XCTest
@testable import ServerBee

@MainActor
final class ServersViewModelErrorTests: XCTestCase {
    func test_fetchServers_setsErrorMessage_onFailure() async {
        let vm = ServersViewModel()
        let client = APIClient.failing(error: APIError.network("boom"))

        await vm.fetchServers(apiClient: client)

        XCTAssertNotNil(vm.errorMessage)
        XCTAssertTrue(vm.servers.isEmpty)
    }

    func test_fetchServers_clearsErrorMessage_onSuccess() async {
        let vm = ServersViewModel()
        vm.errorMessage = "stale"
        let client = APIClient.stub(servers: [
            ServerStatus(id: "1", name: "x", online: true)
        ])

        await vm.fetchServers(apiClient: client)

        XCTAssertNil(vm.errorMessage)
        XCTAssertEqual(vm.servers.count, 1)
    }
}
```

This test depends on test helpers `APIClient.failing(error:)` and `APIClient.stub(servers:)` that exist from Plan 1's test scaffolding. If they don't exist yet, add minimal stubs inside the test file (`extension APIClient { static func failing(...) -> APIClient { ... } }`) wrapping the real initializer with an injected fake transport — implementer's discretion based on the Plan 1 helpers actually present.

- [ ] **Step 2: Run the test to verify it fails**

```bash
xcodebuild test -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16' -only-testing:ServerBeeTests/ServersViewModelErrorTests
```

Expected: FAIL — `errorMessage` property does not exist.

- [ ] **Step 3: Add `errorMessage` and wire it to `fetchServers`**

In `apps/ios/ServerBee/ViewModels/ServersViewModel.swift`, change the class as follows:

```swift
@MainActor
@Observable
final class ServersViewModel {
    var servers: [ServerStatus] = []
    var searchQuery = ""
    var onlineFilter: OnlineFilter = .all
    var isLoading = false
    var isRefreshing = false
    var errorMessage: String?
```

And replace `fetchServers`:

```swift
    func fetchServers(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            servers = try await apiClient.get("/api/servers")
            errorMessage = nil
        } catch {
            errorMessage = String(
                format: String(localized: "Failed to load servers: %@"),
                error.localizedDescription
            )
        }
    }
```

- [ ] **Step 4: Re-run the test**

```bash
xcodebuild test -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16' -only-testing:ServerBeeTests/ServersViewModelErrorTests
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/ViewModels/ServersViewModel.swift apps/ios/ServerBeeTests/ViewModels/ServersViewModelErrorTests.swift
git commit -m "feat(ios): surface fetch error on ServersViewModel"
```

---

## Task 12: Render error state with retry in `ServersListView`

**Files:**
- Modify: `apps/ios/ServerBee/Views/Servers/ServersListView.swift:9-103`

- [ ] **Step 1: Replace the file**

Overwrite `apps/ios/ServerBee/Views/Servers/ServersListView.swift`:

```swift
import SwiftUI

/// The main servers list view, displayed in the Servers tab.
/// Features search, online/offline filter, pull-to-refresh, and navigation to detail.
struct ServersListView: View {
    @Environment(ServersViewModel.self) private var viewModel
    @Environment(\.apiClient) private var apiClient

    var body: some View {
        @Bindable var viewModel = viewModel
        Group {
            if viewModel.isLoading && viewModel.servers.isEmpty {
                loadingView
            } else if let message = viewModel.errorMessage, viewModel.servers.isEmpty {
                errorView(message: message)
            } else if viewModel.servers.isEmpty {
                emptyStateView
            } else {
                serversList
            }
        }
        .navigationTitle(String(localized: "Servers"))
        .searchable(
            text: $viewModel.searchQuery,
            prompt: String(localized: "Search servers...")
        )
        .refreshable {
            if let apiClient {
                await viewModel.refresh(apiClient: apiClient)
            }
        }
        .task {
            if viewModel.servers.isEmpty, let apiClient {
                await viewModel.fetchServers(apiClient: apiClient)
            }
        }
    }

    // MARK: - Subviews

    private var loadingView: some View {
        VStack(spacing: 16) {
            ProgressView()
            Text(String(localized: "Loading servers..."))
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func errorView(message: String) -> some View {
        ContentUnavailableView {
            Label(String(localized: "Couldn't load servers"), systemImage: "exclamationmark.triangle")
        } description: {
            Text(message)
        } actions: {
            Button(String(localized: "Try again")) {
                Task {
                    if let apiClient {
                        await viewModel.fetchServers(apiClient: apiClient)
                    }
                }
            }
            .buttonStyle(.borderedProminent)
        }
    }

    private var emptyStateView: some View {
        ContentUnavailableView {
            Label(String(localized: "No Servers"), systemImage: "server.rack")
        } description: {
            Text(String(localized: "Connect an agent to your server to start monitoring."))
        }
    }

    private var serversList: some View {
        ScrollView {
            LazyVStack(spacing: 12) {
                ServerListHeaderView(
                    filter: Bindable(viewModel).onlineFilter,
                    totalCount: viewModel.servers.count,
                    onlineCount: viewModel.onlineCount
                )
                .padding(.horizontal)

                let filtered = viewModel.filteredServers
                if filtered.isEmpty {
                    noMatchesView
                } else {
                    ForEach(filtered) { server in
                        NavigationLink(value: server) {
                            ServerCardView(server: server)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .padding(.horizontal)
                    }
                }
            }
            .padding(.vertical)
        }
        .background(Color(.systemGroupedBackground))
        .navigationDestination(for: ServerStatus.self) { server in
            ServerDetailView(server: server)
        }
    }

    private var noMatchesView: some View {
        VStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .font(.largeTitle)
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            Text(String(localized: "No matching servers"))
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 60)
        .accessibilityElement(children: .combine)
    }
}

#Preview {
    NavigationStack {
        ServersListView()
    }
    .environment(AuthManager())
    .environment(ServersViewModel())
}
```

- [ ] **Step 2: Build**

```bash
xcodebuild -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16' build
```

Expected: BUILD SUCCEEDED.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Views/Servers/ServersListView.swift
git commit -m "feat(ios): show retry button when servers fetch fails"
```

---

## Task 13: Same error state plumbing for Alerts

**Files:**
- Modify: `apps/ios/ServerBee/ViewModels/AlertsViewModel.swift:1-30`
- Modify: `apps/ios/ServerBee/Views/Alerts/AlertsListView.swift:1-43`
- Create: `apps/ios/ServerBeeTests/ViewModels/AlertsViewModelErrorTests.swift`

- [ ] **Step 1: Write the failing test**

Create `apps/ios/ServerBeeTests/ViewModels/AlertsViewModelErrorTests.swift`:

```swift
import XCTest
@testable import ServerBee

@MainActor
final class AlertsViewModelErrorTests: XCTestCase {
    func test_fetchEvents_setsErrorMessage_onFailure() async {
        let vm = AlertsViewModel()
        let client = APIClient.failing(error: APIError.network("boom"))

        await vm.fetchEvents(apiClient: client)

        XCTAssertNotNil(vm.errorMessage)
        XCTAssertTrue(vm.events.isEmpty)
    }
}
```

- [ ] **Step 2: Run test, confirm failure**

```bash
xcodebuild test -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16' -only-testing:ServerBeeTests/AlertsViewModelErrorTests
```

Expected: FAIL — `errorMessage` property missing.

- [ ] **Step 3: Update `AlertsViewModel`**

Overwrite `apps/ios/ServerBee/ViewModels/AlertsViewModel.swift`:

```swift
import SwiftUI

@MainActor
@Observable
final class AlertsViewModel {
    var events: [MobileAlertEvent] = []
    var isLoading = false
    var isRefreshing = false
    var errorMessage: String?

    func fetchEvents(limit: Int = 50, apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            events = try await apiClient.get("/api/alert-events?limit=\(limit)")
            errorMessage = nil
        } catch {
            errorMessage = String(
                format: String(localized: "Failed to load alerts: %@"),
                error.localizedDescription
            )
        }
    }

    func refresh(apiClient: APIClient) async {
        isRefreshing = true
        await fetchEvents(apiClient: apiClient)
        isRefreshing = false
    }

    /// Called when WebSocket receives an alert_event message -- re-fetch list
    func handleWSAlertEvent(apiClient: APIClient) async {
        await fetchEvents(apiClient: apiClient)
    }
}
```

- [ ] **Step 4: Re-run test**

```bash
xcodebuild test -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16' -only-testing:ServerBeeTests/AlertsViewModelErrorTests
```

Expected: PASS.

- [ ] **Step 5: Update `AlertsListView`**

Overwrite `apps/ios/ServerBee/Views/Alerts/AlertsListView.swift`:

```swift
import SwiftUI

struct AlertsListView: View {
    @Environment(AlertsViewModel.self) private var viewModel
    @Environment(\.apiClient) private var apiClient

    var body: some View {
        Group {
            if viewModel.isLoading && viewModel.events.isEmpty {
                ProgressView(String(localized: "Loading alerts..."))
            } else if let message = viewModel.errorMessage, viewModel.events.isEmpty {
                errorView(message: message)
            } else if viewModel.events.isEmpty {
                ContentUnavailableView {
                    Label(String(localized: "No Alerts"), systemImage: "bell.slash")
                } description: {
                    Text(String(localized: "No alert events to display"))
                }
            } else {
                List {
                    ForEach(viewModel.events) { event in
                        NavigationLink(value: event.alertKey) {
                            AlertEventCardView(event: event)
                        }
                    }
                }
                .listStyle(.plain)
            }
        }
        .navigationTitle(String(localized: "Alerts"))
        .navigationDestination(for: String.self) { alertKey in
            AlertDetailView(alertKey: alertKey)
        }
        .refreshable {
            if let apiClient {
                await viewModel.refresh(apiClient: apiClient)
            }
        }
        .task {
            if viewModel.events.isEmpty, let apiClient {
                await viewModel.fetchEvents(apiClient: apiClient)
            }
        }
    }

    private func errorView(message: String) -> some View {
        ContentUnavailableView {
            Label(String(localized: "Couldn't load alerts"), systemImage: "exclamationmark.triangle")
        } description: {
            Text(message)
        } actions: {
            Button(String(localized: "Try again")) {
                Task {
                    if let apiClient {
                        await viewModel.fetchEvents(apiClient: apiClient)
                    }
                }
            }
            .buttonStyle(.borderedProminent)
        }
    }
}
```

- [ ] **Step 6: Build**

```bash
xcodebuild -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16' build
```

Expected: BUILD SUCCEEDED.

- [ ] **Step 7: Commit**

```bash
git add apps/ios/ServerBee/ViewModels/AlertsViewModel.swift apps/ios/ServerBee/Views/Alerts/AlertsListView.swift apps/ios/ServerBeeTests/ViewModels/AlertsViewModelErrorTests.swift
git commit -m "feat(ios): show retry button when alerts fetch fails"
```

---

## Task 14: Manual backend-down verification

**Files:** none (verification only).

- [ ] **Step 1: Stop the local backend**

If running locally:

```bash
pkill -f serverbee-server || true
```

Otherwise point the app at a deliberately bad host via Settings.

- [ ] **Step 2: Launch the app and pull-to-refresh on each list**

Run the app in the simulator. On the Servers tab, pull-to-refresh and verify:

- Title says "Couldn't load servers"
- Description shows the underlying error
- A "Try again" button is visible and tappable

Do the same on the Alerts tab. Bring the backend back up and tap "Try again" — the list should populate.

No commit needed.

---

## Task 15: Debounce search in `ServersListView`

**Files:**
- Modify: `apps/ios/ServerBee/ViewModels/ServersViewModel.swift`
- Modify: `apps/ios/ServerBee/Views/Servers/ServersListView.swift`
- Create: `apps/ios/ServerBeeTests/ViewModels/ServersViewModelSearchTests.swift`

- [ ] **Step 1: Add `debouncedSearchQuery` to the view model**

In `apps/ios/ServerBee/ViewModels/ServersViewModel.swift`, add the property and switch `filteredServers` to use it:

```swift
    var debouncedSearchQuery = ""

    var filteredServers: [ServerStatus] {
        var result = servers

        if !debouncedSearchQuery.isEmpty {
            let query = debouncedSearchQuery.lowercased()
            result = result.filter { server in
                server.name.lowercased().contains(query) ||
                (server.ipv4?.lowercased().contains(query) ?? false) ||
                (server.ipv6?.lowercased().contains(query) ?? false)
            }
        }

        switch onlineFilter {
        case .all: break
        case .online: result = result.filter { $0.online }
        case .offline: result = result.filter { !$0.online }
        }

        result.sort { a, b in
            if a.online != b.online { return a.online && !b.online }
            return a.name.localizedCaseInsensitiveCompare(b.name) == .orderedAscending
        }

        return result
    }
```

- [ ] **Step 2: Write a failing test for the debounced filter**

Create `apps/ios/ServerBeeTests/ViewModels/ServersViewModelSearchTests.swift`:

```swift
import XCTest
@testable import ServerBee

@MainActor
final class ServersViewModelSearchTests: XCTestCase {
    func test_filteredServers_usesDebouncedSearchQuery() {
        let vm = ServersViewModel()
        vm.servers = [
            ServerStatus(id: "1", name: "alpha", online: true),
            ServerStatus(id: "2", name: "bravo", online: true),
        ]

        vm.searchQuery = "alp"
        // Filtering ignores the live searchQuery until the debounced value is updated.
        XCTAssertEqual(vm.filteredServers.count, 2)

        vm.debouncedSearchQuery = "alp"
        XCTAssertEqual(vm.filteredServers.map(\.name), ["alpha"])
    }
}
```

- [ ] **Step 3: Run the test, confirm pass**

```bash
xcodebuild test -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16' -only-testing:ServerBeeTests/ServersViewModelSearchTests
```

Expected: PASS (the production code already satisfies the contract after Step 1).

- [ ] **Step 4: Wire the debouncer in the view**

In `apps/ios/ServerBee/Views/Servers/ServersListView.swift`, attach a `.task(id:)` modifier to the outer `Group` so the debounced value updates 250 ms after the last keystroke. Append the modifier just below `.task { ... }`:

```swift
        .task(id: viewModel.searchQuery) {
            try? await Task.sleep(for: .milliseconds(250))
            if Task.isCancelled { return }
            viewModel.debouncedSearchQuery = viewModel.searchQuery
        }
```

- [ ] **Step 5: Build & smoke-test**

```bash
xcodebuild -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16' build
```

Expected: BUILD SUCCEEDED. Open Servers tab, type rapidly into the search box: list should not reorder on each keystroke but settle ~250 ms after typing stops.

- [ ] **Step 6: Commit**

```bash
git add apps/ios/ServerBee/ViewModels/ServersViewModel.swift apps/ios/ServerBee/Views/Servers/ServersListView.swift apps/ios/ServerBeeTests/ViewModels/ServersViewModelSearchTests.swift
git commit -m "perf(ios): debounce server search input by 250ms"
```

---

## Task 16: Dynamic Type smoke test at maximum text size

**Files:** none (verification only).

- [ ] **Step 1: Boot the simulator and crank up text size**

In the simulator: Settings → Accessibility → Display & Text Size → Larger Text → enable Larger Accessibility Sizes, slide all the way right.

- [ ] **Step 2: Walk through the app**

Reopen ServerBee and confirm:

- Server cards on the Servers tab still fit the screen (text wraps, no clipping, no horizontal scroll). Status pill text remains legible.
- Tapping a server, the detail metric grid still shows 2 columns with each card padded by the scaled value (no overlap with siblings).
- Settings → Appearance picker rows grow vertically.
- Alerts list rows wrap correctly.

If a view clips or overlaps, file follow-up issues. Reset text size to default afterwards.

No commit needed.

---

## Task 17: Final pass — full test suite + lint

**Files:** none.

- [ ] **Step 1: Run all iOS tests**

```bash
xcodebuild test -project apps/ios/ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 16'
```

Expected: all tests pass, including the three new `*ErrorTests` / `*SearchTests`.

- [ ] **Step 2: Confirm no stray `print(` logging in the touched view models**

```bash
grep -n "print(" apps/ios/ServerBee/ViewModels/ServersViewModel.swift apps/ios/ServerBee/ViewModels/AlertsViewModel.swift
```

Expected: no matches.

- [ ] **Step 3: Confirm no raw `Color(red:` literals remain in source**

```bash
grep -RIn "Color(red:" apps/ios/ServerBee
```

Expected: no matches (the asset-catalog migration replaced them all).

- [ ] **Step 4: If anything failed, fix and commit**

```bash
git add apps/ios/ServerBee
git commit -m "fix(ios): address final ui/a11y audit findings"
```

If everything is clean, no commit needed.

---

## Self-Review Notes

- **#35 covered** by Task 3 (default Picker style)
- **#37 covered** by Tasks 1 + 2 (asset catalog + Swift migration)
- **#38 covered** by Tasks 4, 5, 6, 7, 8, 9, 10 (a11y elements + decorative hiding)
- **#39 covered** by Tasks 4, 5, 16 (`@ScaledMetric`, semantic fonts, manual verification)
- **#40 covered** by Tasks 11, 12, 13, 14 (error state with retry on both lists)
- **#41 covered** by Task 15 (debounced search) and prepped via Task 16 fallout

All steps include exact paths, complete code, exact run commands, and conventional-commit messages without Claude attribution.
