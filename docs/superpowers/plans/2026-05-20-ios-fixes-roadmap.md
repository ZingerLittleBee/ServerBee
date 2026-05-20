# iOS App Fix Roadmap (2026-05-20)

> **For agentic workers:** This roadmap indexes seven implementation plans that together address all 48 issues identified in the iOS code review on 2026-05-20. Execute plans in the documented order to satisfy cross-plan dependencies.

**Scope:** 48 issues across `apps/ios/ServerBee/` (≈3500 LOC Swift) — split into 7 self-contained, independently-shippable plans (102 tasks total).

**Goal:** Move the iOS client from "structurally tidy but not production-ready" (review score ~65%) to TestFlight-ready, with App Store review notes prepared.

---

## Plan Index

| # | Plan | Tasks | Severity | File |
|---|------|-------|----------|------|
| 1 | Realtime Layer / WebSocket Rewrite | 13 | 🔴×4 🟡×4 🟢×1 | [`2026-05-20-ios-plan-1-realtime-websocket.md`](./2026-05-20-ios-plan-1-realtime-websocket.md) |
| 2 | Auth & Concurrency Refactor | 9 | 🔴×2 🟡×6 🟢×1 | [`2026-05-20-ios-plan-2-auth-concurrency.md`](./2026-05-20-ios-plan-2-auth-concurrency.md) |
| 3 | Push Notifications End-to-End | 14 | 🔴×1 🟡×1 🟢×1 | [`2026-05-20-ios-plan-3-push-notifications.md`](./2026-05-20-ios-plan-3-push-notifications.md) |
| 4 | Login, Camera, and Language | 12 | 🔴×4 🟡×1 🟢×1 | [`2026-05-20-ios-plan-4-login-camera-language.md`](./2026-05-20-ios-plan-4-login-camera-language.md) |
| 5 | Models, Formatters, Localization | 14 | 🟡×4 🟢×5 | [`2026-05-20-ios-plan-5-models-formatters.md`](./2026-05-20-ios-plan-5-models-formatters.md) |
| 6 | UI Polish and Accessibility | 17 | 🟢×6 | [`2026-05-20-ios-plan-6-ui-accessibility.md`](./2026-05-20-ios-plan-6-ui-accessibility.md) |
| 7 | Infrastructure & Build Hygiene | 24 | 🔴×1 🟢×5 | [`2026-05-20-ios-plan-7-infrastructure.md`](./2026-05-20-ios-plan-7-infrastructure.md) |
| **Σ** | | **103** | **🔴×12 🟡×16 🟢×20** | |

---

## Dependency Graph

```
Plan 1 (WebSocket)
   │  adds ServerBeeTests target, WebSocketTransport protocol, WebSocketRouter
   ├──► Plan 2 (Auth) — uses ServerBeeTests + URLProtocolStub
   ├──► Plan 3 (Push) — uses ServerBeeTests
   ├──► Plan 4 (Login/Camera) — uses ServerBeeTests + AuthManager @MainActor (from Plan 2)
   ├──► Plan 5 (Models) — uses ServerBeeTests
   ├──► Plan 6 (UI/A11y) — uses ServerBeeTests
   └──► Plan 7 (Infra) — uses ServerBeeTests
```

**Plan 1 is mandatory first** — it introduces the `ServerBeeTests` target and the `WebSocketTransport`/`URLProtocolStub`-style test scaffolding that all other plans rely on. Plan 2 should follow because Plan 4 expects `AuthManager` to be `@MainActor`-isolated. Plans 3/4/5/6/7 are mutually independent after Plans 1+2 land.

---

## Recommended Execution Order

### Phase A — Critical path (TestFlight blockers)

1. **Plan 1** — Realtime/WebSocket — *biggest UX impact, foundational test target*
2. **Plan 2** — Auth/Concurrency — *strict-concurrency safety + first-cold-start UX*
3. **Plan 3** — Push Notifications — *cross-account privacy bug + deep link*
4. **Plan 4** — Login/Camera/Language — *real permission flow, real device names, real language story*

> After Phase A all 🔴 issues are resolved. App is suitable for internal TestFlight.

### Phase B — Production readiness

5. **Plan 5** — Models/Formatters — *contract alignment + locale-aware formatting*
6. **Plan 7** — Infrastructure — *App Store review notes, OSLog, SwiftLint, expanded tests*

> After Phase B the app is App Store-submission ready.

### Phase C — Polish

7. **Plan 6** — UI/Accessibility — *Dynamic Type, VoiceOver, Dark Mode assets, error retry*

> Phase C can be parallelised with B or deferred to a follow-up release.

---

## Issue → Plan Mapping

### 🔴 Blockers (12)
| # | Issue (one-line) | Plan |
|---|------------------|------|
| 1 | `ContentView.onDisappear` closes WS on tab switch | 1 |
| 2 | WS optimistic `.connected` + defeated backoff | 1 |
| 3 | No WebSocket heartbeat ping | 1 |
| 4 | `AuthManager @unchecked Sendable` data race | 2 |
| 5 | `QRScanner` capture session unsafe isolation | 4 |
| 6 | Camera permission denial = black screen, no Settings link | 4 |
| 7 | `apiClient` async-assigned → first-cold-start blank tabs | 2 |
| 8 | Language Picker is a fake switch | 4 |
| 9 | No `ScenePhase` → stale data on foreground return | 1 |
| 10 | Logout doesn't `PushNotificationManager.unregister()` (cross-account leak) | 3 |
| 11 | `NSAllowsArbitraryLoads = true` (ATS off) | 7 |
| 12 | `UIDevice.current.name` returns literal "iPhone" on iOS 16+ | 4 |

### 🟡 Important (16)
| # | Issue | Plan |
|---|-------|------|
| 13 | `MetricsHistoryView` re-instantiates APIClient | 2 |
| 14 | `clearAuth()` selective-reset intent undocumented | 2 |
| 15 | Refresh failure doesn't distinguish 401 vs network | 2 |
| 16 | `ServerStatus.merge` overrides `online` unconditionally | 5 |
| 17 | `RefreshCoordinator` retry semantics | 2 |
| 18 | `WebSocketClient` not actor-isolated | 1 |
| 19 | Close/reconnect race window | 1 |
| 20 | Unused `selectedRange` in `ServerDetailViewModel` | 5 |
| 21 | `NetworkMonitor`/`OfflineBannerView` dead code | 1 |
| 22 | `pushNotificationTapped` notification has no subscriber | 3 |
| 23 | `LoginView.pair()` duplicates URLSession logic | 4 |
| 24 | `CodingKeys` + `convertToSnakeCase` duplicated strategy | 5 |
| 25 | `MobileAlertEvent.id` non-unique across statuses | 5 |
| 26 | `AlertsViewModel.handleWSAlertEvent` never called | 1 |
| 27 | Keychain accessibility class intent uncommented | 2 |
| 28 | Keychain `saveCodable` uses default `JSONEncoder` | 2 |

### 🟢 Suggestions (20)
| # | Issue | Plan |
|---|-------|------|
| 29 | `formatBytes` 1024 base but `KB/MB` labels | 5 |
| 30 | `DateFormatter` created per call | 5 |
| 31 | `formatRelativeTime` hardcoded English | 5 |
| 32 | `ISO8601DateFormatter.shared` requires fractional | 5 |
| 33 | `LoginView` keyboard avoidance on iPhone SE | 4 |
| 34 | `ServerCardView` not `.equatable()` | 1 |
| 35 | `Picker.inline` double header in Settings | 6 |
| 36 | Scattered `print()` instead of `os.Logger` | 7 |
| 37 | Custom Color literals lack Dark Mode variants | 6 |
| 38 | No VoiceOver accessibility labels | 6 |
| 39 | Fixed padding/font not Dynamic Type-friendly | 6 |
| 40 | `ServersViewModel` silently eats errors → misleading empty state | 6 |
| 41 | Search not debounced | 6 |
| 42 | `as! HTTPURLResponse` may crash | 2 |
| 43 | No test target (Plan 1 creates; Plan 7 expands) | 7 |
| 44 | No SwiftLint / swift-format | 7 |
| 45 | `project.yml` `bundleIdPrefix` redundant | 7 |
| 46 | xcstrings keys mix snake & English-as-key | 5 |
| 47 | `aps-environment = development` in Release | 3 |
| 48 | iPad multi-scene deferred | 7 |

---

## Cross-Plan Conventions

All seven plans share these conventions — agentic workers must respect them:

1. **Commit style** — Conventional Commits with `(ios)` scope: `feat(ios):`, `fix(ios):`, `refactor(ios):`, `docs(ios):`, `test(ios):`, `chore(ios):`. Lowercase, imperative, no trailing period.
2. **No Claude attribution** — anywhere (commits, PRs, comments, docs).
3. **TDD red→green→commit** — every behavioural task starts with a failing XCTest, then minimal implementation, then commit. UI/config tasks substitute a manual verification step.
4. **No `print()`** in committed code (Plan 7 enforces a grep gate).
5. **Strict concurrency** — `SWIFT_STRICT_CONCURRENCY: complete`. No new `@unchecked Sendable` or `nonisolated(unsafe)` introductions; existing ones removed by the plans that touch them.
6. **xcodegen-first** — never edit `.xcodeproj` directly; modify `apps/ios/project.yml` then `xcodegen generate`.
7. **English-as-key** xcstrings convention (locked in by Plan 5; later plans must respect).

---

## Verification Gates Between Phases

### After Phase A (TestFlight-ready)
- [ ] All 🔴 issues closed (no `@unchecked Sendable`, no `nonisolated(unsafe)` in app code)
- [ ] WebSocket survives 5-minute backgrounding then foreground (no stale state)
- [ ] Kill backend → app shows OfflineBanner + retry → restart backend → reconnects
- [ ] Tap APNs alert from terminated state → app launches into correct ServerDetail
- [ ] Logout on Device A → device A no longer receives pushes for that account
- [ ] iPhone SE: TOTP field never obscured by keyboard
- [ ] First cold start (with valid token) populates Servers + Alerts tabs immediately
- [ ] `xcodebuild test -scheme ServerBee` passes

### After Phase B (App Store-ready)
- [ ] All 🔴 + 🟡 issues closed
- [ ] `AppStoreReviewNotes.md` exists with ATS justification + demo server
- [ ] Release entitlements use `aps-environment = production`
- [ ] Console.app shows categorised subsystem logs; zero `print()` in source (`rg "\\bprint\\(" apps/ios/ServerBee/` → 0)
- [ ] SwiftLint passes clean on `xcodebuild build`
- [ ] Tests cover ServerStatus.merge / BrowserMessage decode / Formatters / RefreshCoordinator

### After Phase C (Polished)
- [ ] All 🟢 issues closed
- [ ] VoiceOver swipe through Servers tab: every card fully announced
- [ ] Dynamic Type max accessibility size: no overflow / clipping in cards or detail
- [ ] Light/Dark mode visual parity on every screen

---

## Execution Options

Per the writing-plans skill, two execution modes are available:

1. **Subagent-Driven (recommended)** — Dispatch a fresh subagent per task; review between tasks; fast iteration; protects main context.
2. **Inline Execution** — Use `superpowers:executing-plans` skill in the current session; batch execution with checkpoints.

For a body of work this size (~100 tasks), **subagent-driven across the seven plans in priority order** is strongly recommended. Each plan is self-contained — a single subagent can complete one plan end-to-end, then return for review before the next plan starts.

---

## Source Material

- Code review session: 2026-05-20 (this conversation thread)
- Reviewed source: `apps/ios/` at branch `tacoma-v4`, commit `a9a912b7`
- Backend protocol references:
  - `crates/common/src/types.rs:140` (`ServerStatus.online: bool` is non-optional)
  - `crates/common/src/protocol.rs:453-526` (`BrowserMessage` variants)
  - `crates/server/src/router/api/mobile.rs:323-381` (pair endpoint, no 2FA)
  - `crates/server/src/service/apns.rs` (push payload uses `server_id`, `rule_id`)
