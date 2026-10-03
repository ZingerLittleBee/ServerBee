# Combined mobile notification verification

Use this checklist for #205 on one exact candidate SHA. [English operations](../../apps/docs/content/docs/en/push-relay.mdx) and [Chinese operations](../../apps/docs/content/docs/zh/push-relay.mdx) contain configuration ownership, effective signing checks, privacy, troubleshooting and the genuine-device matrix. [The stitched trace](mobile-push-test.md) retains actual Relay/extension boundaries; [security recovery](mobile-push-security.md) retains detailed admission fault cases. These are complementary evidence layers.

## New combined behavior

The three `task_outcomes::lifecycle` cases in `crates/server/tests/mobile_push_tasks/lifecycle.rs` use the real Server HTTP router, mobile sessions, confirmed subscriptions, migrated production-configured SQLite, scheduler/Agent WebSocket task completion, rule-admitted security events, alert evaluation, Rust encryption and production delivery workers. Only the external Relay HTTP and Agent command execution boundaries are fixtures.

| Case | Combined assertion |
| --- | --- |
| `refresh_and_legacy_migration_preserve_all_categories_per_installation` | Two owner installations, another administrator and a member mix subscriptions and permissions. Every real event category enters before and after refresh. Distinct installation content keys and each actual Relay token/environment pair are bound to decrypted account, installation, event, category and original deadline. Refresh invalidates old access without changing registration/key/revision. Stale preference writes do not change delivery. Verified migration excludes the old APNs path while an unrelated legacy recipient remains. All 26 eligible encrypted requests cross the Relay boundary, with original deadlines and terminal ciphertext erasure. |
| `logout_account_replacement_cannot_inherit_any_queued_category` | All five categories queue for two devices. One logs out and signs in as another account at the same installation; it cannot inherit old work or inspect the old test receipt. All five other-device deliveries remain eligible. The replacement registration survives old receipts and cannot subscribe to administrator categories. |
| `role_downgrade_cancels_old_categories_and_allows_new_member_alert_and_test` | HTTP role downgrade blocks every queued old administrator snapshot and current security/task settings. Explicit permitted member subscriptions can admit fresh alerts and tests without receiving old privileged categories. |

`NotificationSetupTests.testPermissionRecoveryAndRefreshKeepAllConfirmedCategoriesUntilAccountReplacement` keeps the real native manager, authentication refresh and login-generation/key ownership while replacing HTTP, permission, token and isolated Keychain boundaries. It verifies all confirmed categories survive denial-to-authorized recovery and refresh, then clears the old key/intent on replacement. HTTP fixtures do not establish Server policy; the combined Server cases above provide that seam.

Retain the existing three-category unsubscribe tests and exclusive encrypted target tests. `EncryptedPushNavigationTests.testCombinedCategoriesKeepExclusiveTargetsForDeliveryAndLateNavigation` covers all five categories, authenticated late taps, strict new-delivery expiry, cross-category target rejection and the actual extension's once-only completion. `CombinedPushTraceTests.testActualForegroundDelegateAndMatchingSecurityWebSocketCompletePresentationOnce` directly invokes the actual foreground delegate with a system-notification archive fixture, routes the same security event through the production WebSocketRouter and SecurityFeedStore, and asserts one completion with the expected options, deduplicated state, no tap and no extra pending system request. The source wiring agrees: `AppDelegate.userNotificationCenter(_:willPresent:withCompletionHandler:)` requests `.banner`, `.badge`, `.sound` once; `ContentView` and `WebSocketRouter` route WS updates to stores without scheduling another notification. The callback test substitutes construction of the system-owned UNNotification, rather than observing APNs presentation. Actual foreground banner count must be observed on the genuine device; no fixture claims that observation.

`PushPreferenceRecoveryTests.testDeniedPermissionRecoveryPreservesDraftAndRejectsOldAccountCompletion` verifies denied-to-authorized recovery retains the confirmed categories, unsaved success/alert draft and visible save error, then suspends registration at the authenticated Server boundary while the account changes. The old completion cannot confirm replacement state, and the old upload is awaited before teardown.

`task_outcomes::lifecycle::trace::stitched_categories_use_actual_relay_and_preserve_external_legacy_delivery` extends the existing real Relay fixture. Seven actual provider transport payloads cover test, alert firing/resolved/rearm, security, task failure and task success; migration excludes the modern installation from legacy selection while an unrelated legacy installation remains. A configured external webhook receives recovery/rearm exactly once. `CombinedPushTraceTests.testStitchedAllServerCategoriesThroughActualExtensionAndAuthenticatedRouter` consumes these exact provider payloads through the actual NSE and cold-launch router, checking identities, exclusive targets, localized non-generic rendering and account replacement rejection. This synthetic APNs boundary does not establish genuine device proof.

## Offline logout and upgrade recovery

`router_mobile` scoped-revocation regressions exercise real verified settings ownership, expiry-hidden originals, repeated/concurrent exact-session absence acknowledgements, replacement-login isolation, malformed IDs, corrupt dangling authority, and refresh-proof bootstrap. Historical outbox rows remain governed by existing eligibility and expiry checks; absence acknowledgement does not promise in-flight recall.

`PendingSessionRevocationTests` keeps production AuthManager/APIClient behavior with isolated storage/HTTP seams: proof-only offline recovery across a fresh AuthManager, legacy fail-closed logout, storage failures/capacity, replacement identity and UI-error fencing, no ambient cookies/credentials, first-settings-write bootstrap, malformed session IDs, and preservation of content/test scope across legacy migration and restart. The private-group test reads actual app-hosted Keychain items. Regenerate the Xcode project for the new Swift files before running the full native test bundle. These fixtures do not establish physical-device Keychain entitlement or APNs behavior.

## Lightweight checks

Use the repository-declared Bun version, Node 24 and existing locked dependencies. Capture stdout/stderr and process exit for each command. Run from the repository root unless stated otherwise:

```sh
bun --filter @serverbee/push-relay test
bun --filter @serverbee/push-relay typecheck
bun --filter @serverbee/docs check:contracts
bun --filter @serverbee/docs types:check
bun run check:agent-navigation
bun run check:integration-targets
bun run test:tooling
python3 tests/check-push-secret-boundaries.py
rustfmt --edition 2024 --config skip_children=true --check crates/server/tests/mobile_push_tasks/lifecycle.rs crates/server/tests/mobile_push_tasks/trace.rs
cd apps/ios
xcodegen generate
```

The repository declares Bun 1.3.4; the Documentation workflow does not explicitly pin Bun. A formatter pass on this new module does not establish a pass for all inherited Rust formatting blocks. Preserve raw failures and exact paths when a broader check is required.

## Full native and generated-contract checks

Run against the exact candidate SHA, using an isolated Cargo target and iOS Simulator. The cloud editor may lack native toolchains; GitHub Actions then owns Rust and iOS execution. Build web assets before checking embedded Server artifacts. No live keys or real device are needed for local fixtures.

```sh
# Build embedded web assets before checking the Server.
bun --filter @serverbee/web build
cargo check --workspace --locked
# New behavior first, then the combined regression suite required by #205.
cargo test -p serverbee-server --test mobile_push_integration task_outcomes::lifecycle --locked
cargo test -p serverbee-server --test mobile_push_integration --test router_mobile --locked
cargo test -p serverbee-server --lib --locked
cargo clippy --workspace --benches --tests --examples --all-features --locked -- -D warnings
# Official generation, inspect zero drift, then client types.
bun --filter @serverbee/web generate:api-types
bun --filter @serverbee/web typecheck
```

Run the existing single synthetic trace and subsequent iOS suite using a unique directory and `SERVERBEE_PUSH_TRACE_DIR` / `TEST_RUNNER_SERVERBEE_PUSH_TRACE_DIR` as described in [the stitched checklist](mobile-push-test.md). The two named trace tests must execute without skipping, within the original 30-minute envelope deadline. Both exact producer commands must run sequentially with the same `SERVERBEE_PUSH_TRACE_DIR` before native tests:

```sh
cargo test -p serverbee-server --test mobile_push_integration encrypted_test_uses_real_relay_admission_transport_and_legacy_migration --locked -- --exact
cargo test -p serverbee-server --test mobile_push_integration task_outcomes::lifecycle::trace::stitched_categories_use_actual_relay_and_preserve_external_legacy_delivery --locked -- --exact
```

Each producer must execute one passing case. Preserve `trace.json` from the original producer. The added producer writes `trace-categories.json`, with `{ "entries": [...] }` containing exactly seven objects with `payload` (actual provider APNs JSON), `content` (Server-decrypted assertion oracle), `registration` (synthetic content-key setup), `user_id`, `installation_id` and `event_id`. Entries cover five kinds and firing/resolved alerts. No Swift-generated replacement input may satisfy this check. The native suite must execute the two existing stitched consumers plus `CombinedPushTraceTests/testStitchedAllServerCategoriesThroughActualExtensionAndAuthenticatedRouter` without skips, and the actual foreground delegate case plus both new permission recovery cases. Minimum new coverage is four Server cases and four native cases, including the new seven-entry stitched consumer. The dedicated destination for this assignment is `platform=iOS Simulator,id=979DE414-28A0-4D7F-8F7F-A90E630F9B5D`.

```sh
cd apps/ios
xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination 'platform=iOS Simulator,id=979DE414-28A0-4D7F-8F7F-A90E630F9B5D' \
  -skipPackagePluginValidation -resultBundlePath "$IOS_RESULTS" test
```

The dispatcher must additionally run the notification/setup/navigation/extension tests in zh-Hans using its isolated test configuration with `AppleLanguages=(zh-Hans)` and `AppleLocale=zh_CN`. Verify those settings actually reach the test host; a shell argument silently ignored by `xcodebuild` is not locale evidence. Record executed/passed/failed/skipped counts and named proofs from `.xcresult`, along with process exit. The iOS CI uses Xcode 16.4; disclose a local toolchain mismatch.

Docs checks follow `apps/docs/README.md` and Documentation CI. In `apps/docs`, run `bun run build`, then start `HOST=127.0.0.1 PORT="$DOCS_PORT" node scripts/start-production.ts` as a dispatcher-owned process with its PID/log recorded. In a separate command run `SERVERBEE_DOCS_BASE_URL="http://127.0.0.1:$DOCS_PORT" bun run check:routes` and `BASE_URL="http://127.0.0.1:$DOCS_PORT" bun run check:browser`. Verify both `/en/docs/push-relay` and `/zh/docs/push-relay`, their mobile/config links, tables and mobile layout. Stop only that owned preview PID. Never terminate a service merely because it occupies a port.

API DTO changes require the official API generator. Inspect generated OpenAPI and client types instead of hand-editing snapshots. Re-run type checks and exact-head CI after incorporating generated changes.

## Evidence record

For each gate record candidate full SHA, command/toolchain, boundary, exit code, executed/passed/failed/skipped counts, named new cases and log/artifact paths. Do not copy earlier-SHA counts into the candidate record. A zero-test success supplies no coverage, and assertions passing with a failed process exit are still a failed check. Mark requested-but-unrun gates PENDING.

Live APNs provider acceptance, foreground/background/terminated presentation and actual taps are NOT RUN until the explicitly authorized environment and signed devices exist. Missing prerequisites are the isolated HTTPS Server/Worker, publisher APNs inputs, matching signed provisioning/entitlements, physical device and both development/distribution builds. The bilingual operations matrix records these independently. No publication, secret installation, deployment, Apple-account or Heeler mutation is authorized by this checklist.
