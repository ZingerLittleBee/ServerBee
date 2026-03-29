# APNs Push Notifications Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable the server to send Apple Push Notifications to registered iOS devices when alerts fire, and add an APNs notification channel type to the web dashboard.

**Architecture:** Server-side adds `a2` crate for APNs HTTP/2 client, a new `Apns` variant in `ChannelConfig`, and push register/unregister endpoints. iOS adds push registration and notification handling. Web adds APNs channel configuration.

**Tech Stack:** Rust (`a2` crate), Swift (UNUserNotificationCenter, UIApplicationDelegate), React

**Spec:** `docs/superpowers/specs/2026-03-29-ios-mvp-design.md` Section 4 + 5.2 + 5.4

---

### Task 1: Server — Add `a2` dependency and APNs service

**Files:**
- Modify: `crates/server/Cargo.toml` — add `a2` crate
- Create: `crates/server/src/service/apns.rs` — APNs push sending
- Modify: `crates/server/src/service/mod.rs` — register module

- [ ] **Step 1: Add a2 dependency**

Add to `crates/server/Cargo.toml` dependencies:
```toml
a2 = "0.10"
```

- [ ] **Step 2: Create apns.rs service**

Implements `send_push(config, device_tokens, context)`:
1. Build `a2::Client` from .p8 key
2. For each token, build notification payload with title, body, sound, badge, custom data (server_id, rule_id)
3. Send all notifications (log errors, clean up invalid tokens)

- [ ] **Step 3: Add `Apns` variant to `ChannelConfig`**

In `crates/server/src/service/notification.rs`, add to the `ChannelConfig` enum:
```rust
Apns {
    key_id: String,
    team_id: String,
    private_key: String,
    bundle_id: String,
    #[serde(default)]
    sandbox: bool,
}
```

Add the APNs dispatch branch in `NotificationService::dispatch`.

- [ ] **Step 4: Add push register/unregister endpoints**

In `crates/server/src/router/api/mobile.rs`, add to the protected router:
- `POST /mobile/push/register` — upsert device_token by installation_id
- `POST /mobile/push/unregister` — delete device_token by installation_id

- [ ] **Step 5: Commit**

```bash
git add crates/server/
git commit -m "feat(server): add APNs push notification support via a2 crate"
```

---

### Task 2: iOS — Push notification registration and handling

**Files:**
- Create: `apps/ios/ServerBee/Services/PushNotificationManager.swift`
- Create: `apps/ios/ServerBee/ServerBee.entitlements`
- Modify: `apps/ios/ServerBee/ServerBeeApp.swift` — AppDelegate adaptor
- Modify: `apps/ios/project.yml` — entitlements + push capability

- [ ] **Step 1: Create entitlements file**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>aps-environment</key>
    <string>development</string>
</dict>
</plist>
```

- [ ] **Step 2: Update project.yml**

Add entitlements and push capability to target settings.

- [ ] **Step 3: Create PushNotificationManager**

Handles:
- Request notification permission
- Register for remote notifications
- Convert device token to hex string
- POST token to `/api/mobile/push/register`
- Handle notification tap → deep link to server detail

- [ ] **Step 4: Update ServerBeeApp with AppDelegate**

Add `@UIApplicationDelegateAdaptor` for APNs callbacks.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/
git commit -m "feat(ios): add APNs push registration and notification handling"
```

---

### Task 3: Web — APNs notification channel configuration

**Files:**
- Modify notification channel settings in the web app to include an "Apple Push Notification" type option

- [ ] **Step 1: Add APNs channel type to notification settings**

In the notification channel creation/edit form, add a new channel type "apns" with fields:
- Key ID (text input)
- Team ID (text input)
- Private Key (.p8 file content, textarea)
- Bundle ID (text input, default "com.serverbee.mobile")
- Sandbox (checkbox, default true for dev)

- [ ] **Step 2: Commit**

```bash
git add apps/web/
git commit -m "feat(web): add APNs notification channel configuration"
```
