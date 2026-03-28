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
   - online/offline and core health summaries
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
    change-password.tsx
  (tabs)/
    _layout.tsx
    servers/
      index.tsx
      [serverId].tsx
      [serverId]/
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
- group by:
  - none
  - server group
  - region

When grouping by server group, the mobile client uses the existing `GET /api/server-groups` endpoint to resolve `group_id` into a display label.

No arbitrary custom grouping, saved views, or dashboard-like segmentation is included in v1.

### Settings Surface

The settings area in v1 is explicitly limited to:

- account/session info
- notification preferences
- theme preference: `system`, `light`, `dark`
- language selection: `en`, `zh`

No advanced layout customization or account-administration settings are included in v1.

### Settings Ownership

- account/session info
  - server-backed display
  - contains the logout action
- notification preferences
  - device-scoped and persisted through device registration
- theme preference
  - local-only app setting, not synced to server
- language selection
  - local-only app setting, not synced to server

### Account Actions

- logout lives on the account/session settings screen
- `Switch account` is implemented as:
  1. execute logout
  2. clear local session state
  3. route back to login
  4. complete the next login
  5. register the current installation for the new account
- v1 does not include multi-account session coexistence on one device

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
  i18n/                   Shared locale JSON resources and translation wiring for en/zh
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

`packages/i18n` becomes the shared source of truth for locale JSON. Both web and mobile should consume the same i18next-compatible resources instead of copying string files into app-local folders.

### Existing Package Boundary Clarification

The repository already contains `packages/auth`, `packages/api`, and `packages/db`, but they are part of a separate TypeScript stack built around Better Auth, oRPC, and Drizzle/Turso. They are **not** the implementation path of the current Rust server.

For this mobile project:

- the system of record remains the Rust backend under `crates/server`
- database migrations for mobile auth/device registration belong under `crates/server/src/migration`
- mobile auth for v1 does **not** reuse `packages/auth`
- `packages/mobile-api` may reuse generated TypeScript types derived from the Rust API contract, but it should not treat `packages/api` as the source of truth for Rust-auth behavior

## Authentication Design

The mobile app should **not** reuse the current browser cookie-session pattern.

### Decision

Use:

- short-lived **access token** as JWT
- refresh token
- SecureStore persistence

### Token Storage Strategy

- **Access token:** stateless JWT, validated without a DB lookup on every request
- **Refresh token:** opaque random token stored in DB for rotation and revocation
- **Signing strategy:** HMAC-signed JWT with a server-managed secret configured specifically for mobile bearer auth

This keeps REST and WebSocket request auth fast while preserving explicit device-session revocation.

### Config Additions

The implementation requires new server-side auth config entries for mobile bearer auth:

- `auth.mobile_jwt_secret`
- `auth.mobile_access_token_ttl_secs`
- `auth.mobile_refresh_token_ttl_secs`

When implemented, the corresponding environment-variable documentation must be updated in `ENV.md` and the configuration docs.

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
     - `user` using the same shape as `MeResponse`
   - errors:
     - `401 invalid_credentials`
     - `401 invalid_totp_code`
     - `422 2fa_required`
2. `POST /api/mobile/auth/refresh`
   - input: `refresh_token`, `installation_id`
   - returns a **rotated** access/refresh pair plus `user` using the same shape as `MeResponse`
   - invalidates the previous refresh token immediately after successful rotation
   - errors:
     - `401 refresh_token_invalid`
     - `401 refresh_token_revoked`
     - `401 refresh_token_expired`
3. `POST /api/mobile/auth/logout`
   - input: `refresh_token`, `installation_id`
   - atomically revokes the refresh-backed device session and clears the push binding for that installation
4. `GET /api/auth/me`
   - authenticated via mobile bearer token after middleware extension
   - returns the current user/session context for cold-launch restore
   - response reuses the existing `MeResponse` contract:
     - `user_id`
     - `username`
     - `role`
     - `must_change_password`

### 2FA Challenge Flow

Mobile v1 keeps the existing backend 2FA semantics and makes the UI flow explicit:

1. User submits username + password
2. If the account requires TOTP and `totp_code` is missing
   - backend returns `422`
   - response code is `2fa_required`
3. The login screen switches into a second-step TOTP prompt
4. The app retries `POST /api/mobile/auth/login` with the same username/password plus `totp_code`
5. If the TOTP code is valid, the backend returns the normal mobile token response
6. If the TOTP code is invalid
   - backend returns `401 invalid_totp_code`
   - app stays on the TOTP step and shows an authentication error without clearing the entered username

### Token Rules

- Access tokens are used for REST and WebSocket auth
- Target access token lifetime: **15 minutes**
- Target refresh token lifetime: **30 days**
- Refresh tokens are scoped per installation/device session, not shared globally across every device
- If refresh fails with `401`, the app clears local credentials and routes to login
- If the backend marks the device session revoked, the app treats it the same as an expired refresh token
- Server-side logout/revocation is **immediate for refresh-token reuse**, but access JWTs may remain valid until their 15-minute expiry
- The app must clear local access tokens immediately on local logout; remote revocation guarantees a maximum 15-minute window unless a future denylist/versioning layer is added

### Auth Middleware Extension

Protected REST routes currently authenticate via cookie session or API key. Mobile support extends the auth chain to:

`session cookie -> API key -> Bearer JWT -> 401`

This same bearer-token check must also be added to the `/api/ws/servers` WebSocket handler, which currently only validates session cookie or API key.

The same bearer-auth path must be accepted by `PUT /api/auth/password` so the forced-password-change flow works for mobile-only sessions.

### WebSocket Authentication

Current browser WebSocket auth is cookie- or API-key-based. Mobile v1 should extend the server-side validator to also accept:

- `Authorization: Bearer <access_token>`

This keeps the mobile WS contract aligned with bearer auth rather than reproducing browser cookie assumptions.

### Launch Flow

1. App starts
2. Splash/loading shell appears
3. Read stored credentials from SecureStore
4. Refresh token if needed
5. If tokens were restored from storage, call `GET /api/auth/me` to fetch current user/session context
6. Route to authenticated app or login flow

After a successful interactive login or refresh response, the returned `user` payload is sufficient; no immediate extra `me` fetch is required in that path.

### Push-Tap Launch Behavior

If the app is opened from a push notification:

1. store the pending deep-link target in memory during app bootstrap
2. attempt normal session restore
3. if auth restore succeeds, navigate to the pending alert detail target
4. if auth restore fails because the session is expired or revoked, route to login and keep the pending target
5. after a successful login, resume the pending alert-detail navigation
6. if the user abandons login, the pending target is discarded and the app remains on the logged-out flow
7. if auth restore fails because of transient network failure, show the offline/retry state and keep the pending target until restore succeeds or the user abandons the launch

### `must_change_password` Policy

If `must_change_password` is `true` in the login, refresh, or `me` response:

- the app blocks entry into the main tabs
- routes the user to `(auth)/change-password.tsx`
- uses the existing `PUT /api/auth/password` endpoint to complete the required change
- only enters the main app after a successful password update and refreshed user/session check

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
- `alert_event`
  - carries `alert_key`, `rule_id`, `server_id`, `status`, `event_at`
  - invalidates the alert list query in the foreground app
  - invalidates the active alert detail query when the `alert_key` matches

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
  3. relies on the WebSocket's initial `full_sync` payload to rebuild authoritative list state
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
4. Tap notification deep-links into alert detail
5. User-facing notification preference controls

### Push Provider Contract

Mobile v1 uses **Expo push tokens** via `expo-notifications`.

- `push_token` in backend contracts means an Expo push token
- backend device registration stores `provider = expo`
- direct APNs/FCM token handling is out of scope for v1
- `locale` in device registration tracks the app’s effective language, not raw OS locale
- if the user changes the app language, the next successful device-registration upsert sends the updated `locale`

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
   - backend keeps the installation record, clears the active push token, and persists the new permission state
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

Settings write path:

- the Settings screen writes notification preference changes immediately via `POST /api/mobile/devices/register`
- the request includes the current `installation_id`, current permission state, current effective locale, and updated preference values
- on cold start or account switch, the screen first hydrates from `GET /api/mobile/devices/current?installation_id=...`
- local UI applies an optimistic update
- if the request fails, the toggles roll back to the last confirmed device-registration state and show retry feedback

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

### Database Schema Additions

The Rust backend needs three new SeaORM-backed tables and corresponding migrations:

1. `mobile_sessions`
   - `id`
   - `user_id` (foreign key -> users.id)
   - `installation_id`
   - `refresh_token_hash`
   - `issued_at`
   - `expires_at`
   - `revoked_at`
   - `last_used_at`
   - active uniqueness key: one non-revoked row per `user_id + installation_id`
2. `mobile_device_registrations`
   - `id`
   - `installation_id` (unique)
   - `user_id` (foreign key -> users.id)
   - `platform`
   - `push_provider`
   - `push_token`
   - `app_version`
   - `locale`
   - `permission_status`
   - `firing_alerts_push`
   - `resolved_alerts_push`
   - `last_seen_at`
   - `disabled_at`
3. `mobile_push_deliveries`
   - `id`
   - `installation_id`
   - `expo_ticket_id`
   - `alert_key`
   - `status`
   - `receipt_error`
   - `sent_at`
   - `checked_at`

`mobile_sessions` is the refresh-token/session table. `mobile_device_registrations` is the push-capable device registry keyed by installation.

Lifecycle rules:

- a single installation may belong to only one user at a time
- logging into a different account on the same installation revokes the old active `mobile_sessions` row and rebinds `mobile_device_registrations.user_id`
- `installation_id` is generated once as a UUID on first run and stored in SecureStore
- if SecureStore survives reinstall, the app reuses the existing `installation_id`
- if SecureStore does not contain an `installation_id`, the app generates a new one and the backend treats it as a new installation
- stale registrations are eventually disabled by last-seen cleanup or invalid token handling
- logout revokes the active `mobile_sessions` row for that installation/user pair and disables the push binding until the next authenticated registration

### Required Additions

1. **Mobile auth endpoints or auth mode support**
   - login that returns token material suitable for native clients
   - refresh endpoint
   - logout/revoke behavior for devices
2. **Device registration**
   - `POST /api/mobile/devices/register` to upsert installation + Expo push token + preference state
   - `POST /api/mobile/devices/unregister` only for in-session token removal / permission-revocation handling without logging out
   - `GET /api/mobile/devices/current?installation_id=` to read backend-confirmed device registration state
3. **Notification deep link payload contract**
   - stable target format for alert/server navigation
4. **Mobile-optimized endpoints required in v1**
   - existing `/api/servers` and `/api/servers/{id}` remain sufficient for server list/detail
   - add `GET /api/mobile/alerts/{alert_key}` so the mobile app can resolve alert detail directly from the canonical `alert_key`

### Push Sending Path

Push delivery should hook into the existing alert evaluation flow instead of inventing a separate trigger source.

The v1 path is:

1. `task/alert_evaluator.rs` runs rule evaluation
2. `AlertService` detects a triggered or resolved state change after persistence
3. New Rust-side `MobilePushService` resolves eligible `mobile_device_registrations`
4. `MobilePushService` sends Expo push batches via `reqwest` to `https://exp.host/--/api/v2/push/send`
5. Expo ticket IDs are persisted in `mobile_push_deliveries`
6. A receipt-check background job polls Expo receipts and marks stale device registrations disabled when `DeviceNotRegistered` is returned

Design constraints:

- batch requests within Expo API limits
- send only to installations whose device-level preference matches the event type
- retry transient HTTP/network failures with bounded backoff
- record permanent invalidation outcomes by disabling stale device registrations when Expo receipts indicate `DeviceNotRegistered`
- push send + receipt processing must be idempotent for the same `installation_id + alert_key + event_at`

The first implementation can remain best-effort, but it must have explicit retry and token-invalidation behavior.

### Device Registration Endpoint Shapes

`POST /api/mobile/devices/register`

- requires a valid mobile bearer session
- request:
  - `installation_id`
  - `platform`
  - `push_provider = expo`
  - optional `push_token`
  - `app_version`
  - `locale`
  - `permission_status`
  - `firing_alerts_push`
  - `resolved_alerts_push`
- response:
  - `device_registration_id`
  - `installation_id`
  - `push_provider`
  - `registered_at`

Ownership rules:

- server derives `user_id` from the bearer session, never from client input
- `installation_id` may be created for the authenticated user or rebound during an explicit account switch on the same device
- a register request cannot attach an installation to a different user without revoking the previous installation-bound session first
- if `push_token` is omitted, the request is treated as a permission-state update and clears any previously stored push token for that installation

Account-switch handshake:

1. user taps `Switch account`
2. app calls `POST /api/mobile/auth/logout`
3. app clears local tokens and returns to login
4. user completes login for the new account
5. app calls `POST /api/mobile/devices/register` for the same `installation_id`
6. if the backend still sees the installation as actively bound elsewhere, it returns `409 installation_rebind_required`
7. the client surfaces retry guidance and retries registration only after local logout state is clean

Failure modes:

- `401 invalid_bearer_session`
- `409 installation_rebind_required` when the installation is still actively bound to another user session
- `200` permission-state update accepted when `push_token` is omitted and `permission_status` changed
- `200` upsert accepted when the same authenticated installation refreshes its token or metadata

`POST /api/mobile/devices/unregister`

- requires a valid mobile bearer session
- request:
  - `installation_id`
- response:
  - `ok`

Ownership rules:

- the server only unregisters the `installation_id` bound to the authenticated user
- unregistering another user’s installation returns authorization failure
- this endpoint does **not** revoke the mobile auth session; it only clears device-push state while the user remains signed in

`GET /api/mobile/devices/current?installation_id=...`

- requires a valid mobile bearer session
- response:
  - `installation_id`
  - `permission_status`
  - `firing_alerts_push`
  - `resolved_alerts_push`
  - `has_push_token`
  - `locale`
- used by the Settings screen to restore backend-confirmed notification state after auth restore or account switch
- if no device row exists yet, returns `200` with default values:
  - `permission_status = unknown`
  - `firing_alerts_push = true`
  - `resolved_alerts_push = false`
  - `has_push_token = false`

### Quick Actions

`Quick actions` in mobile v1 are explicitly limited to **client-side or read-only shortcuts**. They do not mutate server state.

The v1 set is:

1. Refresh current screen data
2. Open related server from an alert
3. Copy server ID or primary IP

Anything requiring backend-side operational control, such as acknowledging alerts, silencing rules, upgrades, or remote execution, is out of scope for v1.

### Authorization Model for Quick Actions

- All v1 quick actions are available to any authenticated user who can view the source data
- No admin-only quick action surface exists in v1
- Because the v1 set is read-only/client-local, no new write authorization path is required

### Preferred Constraint

Add the minimum server surface needed for mobile instead of creating a parallel backend for the app.

### Main Screen API Contracts

#### Server List

- request:
  - `GET /api/servers`
  - `GET /api/server-groups` for server-group labels when grouped rendering is active
- primary fields used in v1 list:
  - `id`
  - `name`
  - `group_id`
  - `region`
  - `country_code`
  - `online`
  - `cpu`
  - `mem_used`
  - `mem_total`
  - `disk_used`
  - `disk_total`
  - `features`
  - `capabilities`
- sorting:
  - default order is `online desc`, then `name asc`
- paging:
  - none in v1; the full visible server set is loaded once and filtered client-side
- empty state:
  - show "No servers connected" with retry
- error state:
  - expired auth -> route to login
  - network failure -> inline retry state

#### Server Detail

- requests:
  - `GET /api/servers/{id}`
  - `GET /api/servers/{id}/records?from&to&interval`
- detail screen sections:
  - overview summary
  - live metric cards
  - history charts
  - related alerts entry point
- error state:
  - `404` -> server not found state
  - network failure -> retry state

#### Alert List

- request:
  - `GET /api/alert-events?limit=50`
- response item fields used in v1:
  - `rule_id`
  - `rule_name`
  - `server_id`
  - `server_name`
  - `status`
  - `event_at`
  - `resolved_at`
  - `count`
- sorting:
  - `event_at desc`
- paging:
  - no cursor pagination in v1; pull-to-refresh reloads the latest 50 events
- empty state:
  - show "No recent alerts"
- error state:
  - inline retry state

#### Alert Detail

- request:
  - `GET /api/mobile/alerts/{alert_key}`
- semantic meaning:
  - returns the **current alert context** for the `rule_id + server_id` pair represented by `alert_key`
  - it is not a historical event replay endpoint
  - if the alert currently has an unresolved state, the endpoint returns that active aggregate
  - otherwise it returns the latest resolved summary for that alert key
- response fields:
  - `alert_key`
  - `rule_id`
  - `rule_name`
  - `server_id`
  - `server_name`
  - `status`
  - `event_at`
  - `resolved_at`
  - `count`
  - `related_metrics_snapshot`
  - `server_route`
- failure modes:
  - `404` if the alert key cannot be resolved
  - fallback UI routes the user back to the alert list if lookup fails

`related_metrics_snapshot` schema:

```json
{
  "captured_at": "2026-03-28T10:00:00Z",
  "cpu": 82.1,
  "mem_used": 17179869184,
  "mem_total": 34359738368,
  "disk_used": 214748364800,
  "disk_total": 536870912000,
  "load1": 2.4,
  "load5": 2.1,
  "load15": 1.8,
  "net_in_speed": 1048576,
  "net_out_speed": 524288
}
```

`server_route` schema:

```json
{
  "pathname": "/servers/[serverId]",
  "params": {
    "serverId": "server_456"
  }
}
```

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

### Historical Metrics API

Mobile v1 reuses the existing server history endpoint:

- `GET /api/servers/{id}/records?from=<utc>&to=<utc>&interval=<raw|hourly|auto>`

The current backend behavior is already suitable for the first mobile version:

- `auto`
  - uses raw records for ranges up to 24 hours
  - uses hourly aggregates for ranges over 24 hours

The mobile metrics screen should start with fixed windows:

- `1h`
- `6h`
- `24h`
- `7d`

Expected interval usage:

- `1h`, `6h`, `24h` -> `auto` resolves to raw
- `7d` -> `auto` resolves to hourly

This keeps payload size reasonable on mobile networks without inventing a separate history API in v1.

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
- alert detail -> related server navigation
- background/foreground reconnection behavior
- weak network behavior

### Backend Integration Coverage

The implementation plan must include backend integration tests for:

- mobile login -> refresh -> logout
- 2FA-required mobile login flow
- device registration upsert and account-switch rebind
- WebSocket bearer auth on `/api/ws/servers`
- Expo push invalidation handling for stale device registrations

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
