# ServerBee iOS

SwiftUI client for the ServerBee server. Generated via `xcodegen`.

## Build

```
cd apps/ios
xcodegen generate
open ServerBee.xcodeproj
```

## Configurations

| Config  | Entitlements file                          | aps-environment |
| ------- | ------------------------------------------ | --------------- |
| Debug   | `ServerBee/ServerBee.Debug.entitlements`   | `development`   |
| Release | `ServerBee/ServerBee.Release.entitlements` | `production`    |

`xcodegen` writes per-configuration `CODE_SIGN_ENTITLEMENTS` settings from
`project.yml`. Edit `project.yml` (not the generated `.xcodeproj`) when
changing entitlements.

## App Store submission checklist

Before archiving for App Store / TestFlight:

1. **Configuration:** Product → Scheme → Edit Scheme → Archive → Build Configuration = `Release`.
2. **Entitlements:** confirm the archive embeds the production entitlements:
   ```
   codesign -d --entitlements - <Archive>.xcarchive/Products/Applications/ServerBee.app
   ```
   Expect `<key>aps-environment</key><string>production</string>`.
3. **APNs key/cert:** the matching App ID in the Apple Developer portal must
   have the **APNs Production** key/certificate enabled, and the backend
   `apns` config (see `crates/server/src/service/apns.rs`) must reference the
   same key (`SERVERBEE_APNS__KEY_PATH`) and team id.
4. **Push tap deep link:** send a TestFlight build push payload with
   `server_id` custom data; verify the app opens to `ServerDetailView`.
5. **Logout hygiene:** sign out → confirm the next push to this device does
   NOT arrive (token unregistered server-side).

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

ATS is fully disabled (`NSAllowsArbitraryLoads`) because users connect to
self-hosted ServerBee servers on arbitrary IPs and domains (including LAN-only
deployments that cannot obtain publicly-trusted TLS certificates). The
`InsecureURLBanner` view surfaces a runtime warning on the Login and Settings
screens whenever the configured URL begins with `http://`. The full rationale
and mitigations live in `AppStoreReviewNotes.md`.

### Linting

- **SwiftLint** runs as a Swift Package build tool plugin
  (`SimplyDanny/SwiftLintPlugins`) on every Xcode build. Config:
  `.swiftlint.yml`. First-run validation requires Xcode to trust the package
  plugin (Settings → Trust & Manage Plug-ins); for headless CI builds pass
  `-skipPackagePluginValidation`.
- **swift-format** runs via `./scripts/format-check.sh` (intended for CI).
  The script honors `.swift-format` at the iOS app root and runs without
  `--strict` so style diagnostics surface without failing the run.
