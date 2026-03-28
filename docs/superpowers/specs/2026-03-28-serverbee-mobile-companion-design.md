# ServerBee Mobile Companion App

**Date:** 2026-03-28
**Scope:** Add a new Expo-based iOS/Android app for high-frequency mobile monitoring flows. No desktop-web parity goal in v1.

## Problem

ServerBee currently ships a browser-first React SPA and a Rust backend, but it does not have a dedicated mobile app. The existing web implementation is not a good candidate for direct mobile reuse because several key screens depend on browser-specific capabilities:

- `xterm` terminal rendering
- Monaco-based file editing
- drag-and-drop dashboard layout editing
- browser cookie session behavior
- `window`, `navigator`, `ResizeObserver`, and other browser APIs

Trying to force the current web UI into a mobile shell would produce weak native UX and slow down delivery. At the same time, ServerBee is a monitoring product, which means mobile value is concentrated in viewing status, receiving alerts, and taking a small number of fast actions.

## Decision

Build a dedicated **mobile companion app** with **Expo + React Native** for **iOS and Android only**.

The mobile app is intentionally scoped as a companion experience:

- optimized for monitoring, alert response, and fast inspection
- native navigation and push-first workflows
- shared data contracts with the backend where practical
- no attempt to reproduce desktop-style terminal, file editing, or dashboard authoring in v1

## Goals

1. Ship a production-viable mobile app that can go through internal testing first and then move quickly to App Store / Google Play release.
2. Cover the highest-value mobile use cases:
   - login and session restore
   - server list and server detail
   - real-time core metrics
   - alert list and alert detail
   - push notifications
   - a small set of client-side quick actions
3. Reuse backend APIs, WebSocket event models, and shared TypeScript data contracts where that reduces duplication without forcing UI sharing.
4. Fit cleanly into the existing Bun workspace + Turborepo monorepo.

## Non-Goals

1. No v1 support for:
   - terminal sessions
   - remote file management or editing
   - dashboard drag-and-drop editing
   - desktop-class configuration workflows
   - Docker-heavy operational control surfaces
2. No web-to-mobile UI sharing strategy.
3. No bare React Native setup unless Expo becomes a hard blocker.

## Approaches Considered

### 1. Companion App

Build a focused native app for mobile-first flows only.

**Pros**

- Best fit for a monitoring product on phone-sized screens
- Fastest route to internal testing and public release
- Cleanest UX
- Lowest architectural risk

**Cons**

- Mobile feature set is narrower than the web app

### 2. Full Mobile Console

Treat mobile as a near-equal second primary client and attempt to cover most web features.

**Pros**

- Strong long-term parity story

**Cons**

- Large scope increase
- Browser-dependent features become expensive to re-implement natively
- Slower route to release

### 3. Hybrid Native + WebView Escape Hatches

Use native screens for core flows and WebView for complex existing web pages.

**Pros**

- Faster short-term feature coverage than a full native rewrite

**Cons**

- Inconsistent UX
- Messier auth and navigation boundaries
- Higher maintenance cost

### Recommendation

Choose **Approach 1: Companion App**.

## Product Scope

### v1 In Scope

- Authentication
- Session restore
- Server list
- Server detail
- Real-time core metrics
- Alert list
- Alert detail
- Push notification registration and deep linking
- Client-side quick actions with fixed scope
- App settings

### v1 Out of Scope

- Terminal
- File browser/editor
- Dashboard editor
- Broad server administration workflows
- Web parity for every feature area

## Information Architecture

Top-level navigation should be minimal and native:

1. **Servers**
   - default landing area
   - search/filter/grouping
   - online/offline and alert summaries
2. **Alerts**
   - alert inbox
   - severity/status context
   - alert detail and related-server navigation
3. **Settings**
   - account/session
   - notification preferences
   - theme and app-level settings

### Primary Screens

```text
app/
  _layout.tsx
  (auth)/
    login.tsx
  (tabs)/
    _layout.tsx
    servers/
      index.tsx
      [serverId].tsx
      metrics.tsx
    alerts/
      index.tsx
      [alertKey].tsx
    settings/
      index.tsx
      notifications.tsx
      account.tsx
  modal/
    quick-action.tsx
    alert-filter.tsx
```

This follows Expo Router conventions rather than custom React Navigation setup.

### List Behavior Constraints

The server list in v1 is intentionally limited to:

- search by server name and IP text
- filter by online state: `all`, `online`, `offline`
- filter by alert state: `all`, `has_active_alerts`, `no_active_alerts`
- group by:
  - none
  - server group
  - region

No arbitrary custom grouping, saved views, or dashboard-like segmentation is included in v1.

### Settings Surface

The settings area in v1 is explicitly limited to:

- account/session info
- notification preferences
- theme preference: `system`, `light`, `dark`
- language selection: `en`, `zh`

No advanced layout customization or account-administration settings are included in v1.

## Recommended Technology Stack

- **Framework:** Expo
- **Routing:** Expo Router
- **Server-state caching:** TanStack Query
- **Light app state:** Zustand
- **Sensitive storage:** `expo-secure-store`
- **Push notifications:** `expo-notifications`
- **Build/release:** EAS Build

### Why This Stack

It matches the current team’s React/TypeScript strengths, aligns with the monorepo structure, and avoids manual native project setup. It also follows current Expo/React Native best practices:

- file-based routing via Expo Router
- secure token storage via SecureStore
- push integration via Expo Notifications
- managed build pipeline via EAS

## Monorepo Structure

Add a new app and a small set of mobile-focused shared packages:

```text
apps/
  mobile/                 Expo app, routes, native config, screens

packages/
  mobile-api/             API client, DTO adapters, query keys, WS message parsing
  mobile-i18n/            Shared locale resources and translation wiring for en/zh
```

Existing packages such as `packages/api` may be reused where they contain genuinely shared contracts, but the mobile app should not depend on the web UI package or browser-oriented modules.

### Sharing Rule

Share:

- DTOs
- shared domain types
- formatting helpers where platform-neutral
- query keys and networking helpers
- translation resources when practical

Do not share:

- web components
- browser hooks
- web auth assumptions
- browser-only editor/terminal/dashboard abstractions

### Localization Scope

Mobile v1 ships with the same two supported locales already present in the web app:

- English (`en`)
- Chinese (`zh`)

`packages/mobile-i18n` becomes the source of truth for mobile strings. It may be seeded from the existing web locale JSON files, but mobile-only copy lives in the mobile package rather than importing browser app modules directly.

## Authentication Design

The mobile app should **not** reuse the current browser cookie-session pattern.

### Decision

Use:

- short-lived **access token**
- refresh token
- SecureStore persistence

### Concrete v1 Contract

Add a mobile-specific auth namespace instead of overloading the cookie-oriented browser endpoints:

1. `POST /api/mobile/auth/login`
   - input: `username`, `password`, optional `totp_code`, `installation_id`
   - returns:
     - `access_token`
     - `access_expires_in_secs`
     - `refresh_token`
     - `refresh_expires_in_secs`
     - `token_type` = `Bearer`
     - `user`
2. `POST /api/mobile/auth/refresh`
   - input: `refresh_token`, `installation_id`
   - returns a **rotated** access/refresh pair
   - invalidates the previous refresh token immediately after successful rotation
3. `POST /api/mobile/auth/logout`
   - input: `refresh_token`, `installation_id`
   - revokes the current device session and unregisters its push binding

### 2FA Challenge Flow

Mobile v1 keeps the existing backend 2FA semantics and makes the UI flow explicit:

1. User submits username + password
2. If the account requires TOTP and `totp_code` is missing
   - backend returns `422`
   - response code is `2fa_required`
3. The login screen switches into a second-step TOTP prompt
4. The app retries `POST /api/mobile/auth/login` with the same username/password plus `totp_code`
5. If the TOTP code is valid, the backend returns the normal mobile token response
6. If the TOTP code is invalid, the app stays on the TOTP step and shows an authentication error without clearing the entered username

### Token Rules

- Access tokens are used for REST and WebSocket auth
- Target access token lifetime: **15 minutes**
- Target refresh token lifetime: **30 days**
- Refresh tokens are scoped per installation/device session, not shared globally across every device
- If refresh fails with `401`, the app clears local credentials and routes to login
- If the backend marks the device session revoked, the app treats it the same as an expired refresh token

### WebSocket Authentication

Current browser WebSocket auth is cookie- or API-key-based. Mobile v1 should extend the server-side validator to also accept:

- `Authorization: Bearer <access_token>`

This keeps the mobile WS contract aligned with bearer auth rather than reproducing browser cookie assumptions.

### Launch Flow

1. App starts
2. Splash/loading shell appears
3. Read stored credentials from SecureStore
4. Refresh token if needed
5. Fetch current user/session context
6. Route to authenticated app or login flow

### Why

Native apps should not depend on browser cookie transport assumptions. A bearer-token flow is clearer, more secure for this environment, and easier to reason about across REST, WebSocket, and notification flows.

## Data and Realtime Design

### REST

REST remains the source of truth for:

- authentication
- initial list/detail payloads
- alert detail
- settings

### WebSocket

WebSocket remains the source for incremental updates:

- server online/offline changes
- core metrics changes
- capability changes when relevant
- alert-driven live refresh triggers

### Concrete v1 WebSocket Contract

Mobile v1 consumes the existing browser-style server stream and intentionally supports only the subset needed for the companion scope:

- `full_sync`
  - replaces the `['servers']` cache
- `update`
  - merges into `['servers']`
  - updates `['servers', serverId]` when that detail screen is active
- `server_online` / `server_offline`
  - toggles online state in list/detail caches
- `capabilities_changed`
  - updates capability metadata in list/detail caches
- `agent_info_updated`
  - updates protocol/version metadata in detail cache

Messages that only matter for browser-heavy features, such as terminal or Docker-heavy streaming, are ignored by mobile v1 unless they directly support an in-scope screen.

### Client Cache

TanStack Query owns server-state caching. WebSocket messages merge into the query cache rather than creating a second parallel state model.

Zustand should be limited to app UI concerns such as:

- active filters
- tab-local UI state
- ephemeral view preferences

### Reconnect and Offline Behavior

- On foreground app usage, the WebSocket stays connected
- When the app backgrounds, the socket may close; on foreground resume, the app:
  1. verifies session validity
  2. reconnects the socket
  3. invalidates `servers` and alert-summary queries once
  4. resumes incremental updates
- If the socket drops unexpectedly while the app is foregrounded, the client retries with backoff
- If reconnect fails because the access token expired, the app performs one refresh attempt before forcing login
- Mobile v1 does **not** include an offline mutation queue
- On cold offline startup, the app may show last-known cached read data if present; otherwise it shows an offline state with retry

## Notification Design

Push notifications are a core part of the mobile value proposition.

### v1 Requirements

1. Device token registration from the app
2. Backend storage of device notification registrations
3. Push on alert-created and alert-resolved events
4. Tap notification deep-links into:
   - alert detail when the payload refers to an alert
   - related server detail when appropriate
5. User-facing notification preference controls

### Push Provider Contract

Mobile v1 uses **Expo push tokens** via `expo-notifications`.

- `push_token` in backend contracts means an Expo push token
- backend device registration stores `provider = expo`
- direct APNs/FCM token handling is out of scope for v1

### Device Registration Lifecycle

Each app install creates and persists a stable `installation_id`.

The lifecycle is:

1. App launch/login
   - app obtains or restores `installation_id`
   - app requests notification permission
   - app registers permission state with the backend
2. If a push token exists
   - app upserts the device registration with:
     - `installation_id`
     - `platform`
     - `push_token`
     - `app_version`
     - `locale`
     - `permission_status`
3. If the push token changes
   - the same installation record is updated in place
4. If permission is denied or revoked
   - backend keeps the installation record but clears the active push token
5. On logout
   - backend unregisters the installation’s push token and revokes the mobile session

### Duplicate Token Handling

- `installation_id` is the primary identity for a device registration
- If the same push token appears for multiple records, the latest authenticated installation wins and older bindings are disabled
- Re-registering the same installation is an upsert, not a duplicate row

### Notification Preference Model

v1 keeps preferences intentionally small:

- `firing_alerts_push` = default `true`
- `resolved_alerts_push` = default `false`

These preferences are owned **per installation/device**, not globally per user account. A user may keep different push settings on different phones or tablets.

No per-rule or per-server mobile notification matrix is included in v1.

### Notification Deep-Link Payload Schema

Push payloads must use a single stable v1 schema:

```json
{
  "target_type": "alert_event",
  "alert_key": "rule_123:server_456",
  "rule_id": "rule_123",
  "server_id": "server_456",
  "status": "firing",
  "event_at": "2026-03-28T10:00:00Z"
}
```

Rules:

- `target_type` is `alert_event` for alert pushes in v1
- `alert_key` is the canonical mobile alert identifier in v1 and is defined as `${rule_id}:${server_id}`
- `status` is `firing` or `resolved`
- client route is `alerts/[alertKey].tsx`
- client opens alert detail using `alert_key`
- alert detail includes a jump to the related server screen
- if payload validation fails, app falls back to the alerts list

### Out of Scope

- advanced multi-device notification policy UI
- deep action-button workflows for every alert type
- highly customized automation rules in the mobile client

## Backend Changes Required

The backend can be reused heavily, but mobile introduces explicit new requirements.

### Required Additions

1. **Mobile auth endpoints or auth mode support**
   - login that returns token material suitable for native clients
   - refresh endpoint
   - logout/revoke behavior for devices
2. **Device registration**
   - `POST /api/mobile/devices/register` to upsert installation + Expo push token + preference state
   - `POST /api/mobile/devices/unregister` on logout or token removal
3. **Notification deep link payload contract**
   - stable target format for alert/server navigation
4. **Optional mobile-optimized summary endpoints**
   - only if current responses are too web-shaped for efficient mobile use

### Quick Actions

`Quick actions` in mobile v1 are explicitly limited to **client-side or read-only shortcuts**. They do not mutate server state.

The v1 set is:

1. Refresh current screen data
2. Open related server from an alert
3. Open filtered alerts for the current server
4. Copy server ID or primary IP

Anything requiring backend-side operational control, such as acknowledging alerts, silencing rules, upgrades, or remote execution, is out of scope for v1.

### Authorization Model for Quick Actions

- All v1 quick actions are available to any authenticated user who can view the source data
- No admin-only quick action surface exists in v1
- Because the v1 set is read-only/client-local, no new write authorization path is required

### Preferred Constraint

Add the minimum server surface needed for mobile instead of creating a parallel backend for the app.

## UX Boundaries

The mobile app should feel native and focused, not like a reduced browser shell.

## Server Detail Data Contract

`Real-time core metrics` in v1 are explicitly limited to the following server detail data:

### Summary Fields

- online/offline
- last active timestamp
- server name
- ipv4
- ipv6
- derived `primary_ip` = `ipv4` if present, else `ipv6`, else `null`
- region / country
- OS
- CPU name
- uptime

### Live Metric Cards

- CPU usage
- memory used / total
- disk used / total
- network in speed
- network out speed
- load 1 / 5 / 15
- process count
- TCP connection count
- UDP connection count

### History Charts

The `metrics` screen in v1 is limited to these time-series charts:

- CPU
- memory
- disk
- network in / out

No custom widget dashboards, chart editing, or parity with the web dashboard builder is part of v1.

### Design Principles

1. Default to one-hand-friendly, inspection-first screens
2. Prefer summary -> detail navigation over dense dashboards
3. Push users into the exact alert/server context they need
4. Keep quick actions narrow and safe
5. Avoid any screen that requires desktop-style sustained interaction

## Testing Strategy

### Automated

Target automated coverage for:

- auth/session restore logic
- token refresh behavior
- query hooks
- WebSocket-to-query-cache merging
- notification deep link parsing

### Manual

Target manual verification for:

- cold start with valid session
- expired token recovery
- login/logout
- notification permission granted/denied
- push open -> alert detail
- push open -> server detail
- background/foreground reconnection behavior
- weak network behavior

### High-Risk Acceptance Matrix

| Scenario | Expected v1 behavior |
|----------|----------------------|
| Expired refresh token | App clears session, shows login, does not loop refresh requests |
| Revoked device session | Treated the same as refresh failure; local credentials cleared |
| Notification permission denied | App remains usable; settings show push disabled; backend stores permission state without active token |
| Cold startup while offline | App shows last-known cached read data if available, otherwise offline empty state with retry |
| WebSocket reconnect after backgrounding | App reconnects on foreground, invalidates summary queries once, then resumes live updates |

### Not a v1 Priority

Heavy upfront investment in full E2E mobile automation is not required before the first internal testing cycle. The focus is stable core flows first.

## Release Plan

Release target is:

1. internal testing first
2. public release soon after stabilization

### Delivery Sequence

1. Initialize `apps/mobile` with Expo CLI
2. Establish routing, auth shell, and shared packages
3. Ship login + session restore
4. Ship server list/detail + realtime updates
5. Ship alerts + push notifications
6. Run internal testing on iOS and Android
7. Fix stability, permission, and release-blocking issues
8. Prepare App Store / Google Play metadata and submit

## Risks

1. **Auth mismatch risk**
   - current web auth assumptions may leak into mobile unless the token model is made explicit
2. **Scope creep**
   - terminal/file/dashboard parity requests can quickly derail the first release
3. **Notification backend gap**
   - mobile value is reduced if push registration and delivery are delayed
4. **Over-sharing code**
   - trying to reuse browser-first abstractions would slow the app and muddy boundaries

## Success Criteria

The design is successful if the first mobile release can:

1. let a signed-in user restore session without friction
2. show server health and alert state quickly
3. react to live data changes without manual refresh dependency
4. deep-link from push notifications into the right context
5. move cleanly from internal testing to public store release

## Out of Scope Follow-Ups

Potential later phases, not part of this design:

- richer quick actions
- offline-friendly detail caching expansion
- mobile widgets
- Apple Watch / Wear OS extensions
- advanced admin workflows
- carefully selected remote-operation capabilities
