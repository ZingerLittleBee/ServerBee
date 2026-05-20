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
