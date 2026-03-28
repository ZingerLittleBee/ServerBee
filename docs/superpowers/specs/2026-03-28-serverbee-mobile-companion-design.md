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
   - a small set of quick actions
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
- Quick actions with narrow scope
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
   - alert detail and acknowledgment flow
3. **Settings**
   - account/session
   - notification preferences
   - appearance and app-level settings

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
      [alertId].tsx
    settings/
      index.tsx
      notifications.tsx
      account.tsx
  modal/
    quick-action.tsx
    alert-filter.tsx
```

This follows Expo Router conventions rather than custom React Navigation setup.

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
  mobile-i18n/            Shared locale resources and translation wiring
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

## Authentication Design

The mobile app should **not** reuse the current browser cookie-session pattern.

### Decision

Use:

- short-lived **access token**
- refresh token
- SecureStore persistence

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
- quick-action requests

### WebSocket

WebSocket remains the source for incremental updates:

- server online/offline changes
- core metrics changes
- capability changes when relevant
- alert-driven live refresh triggers

### Client Cache

TanStack Query owns server-state caching. WebSocket messages merge into the query cache rather than creating a second parallel state model.

Zustand should be limited to app UI concerns such as:

- active filters
- tab-local UI state
- ephemeral view preferences

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
   - register push token
   - update/delete push token when app session changes
3. **Notification deep link payload contract**
   - stable target format for alert/server navigation
4. **Optional mobile-optimized summary endpoints**
   - only if current responses are too web-shaped for efficient mobile use

### Preferred Constraint

Add the minimum server surface needed for mobile instead of creating a parallel backend for the app.

## UX Boundaries

The mobile app should feel native and focused, not like a reduced browser shell.

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
