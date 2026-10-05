# Encrypted current-installation notification verification

This checklist retains the #199 encrypted trace, #200 durable retries and subsequent category/regression boundaries. Use [combined integration verification](mobile-push-integration.md) for #205 and the linked bilingual operations runbook for genuine-device acceptance. Never use Heeler's Relay, production
credentials, a user's existing Simulator, or a live account for fixtures.

## Local stitched path

Prerequisites: installed repository Bun dependencies, Rust toolchain,
Xcode, XcodeGen and a dedicated iOS Simulator. The assigned dispatcher serializes
Cargo/Xcode commands. The task agent may run the lightweight Bun checks.

From the repository root:

```sh
bun install --frozen-lockfile --ignore-scripts
bun --filter @serverbee/push-relay typecheck
bun --filter @serverbee/push-relay test
bun --filter @serverbee/docs check:contracts
bun run check:agent-navigation
bun run check:integration-targets
python3 tests/check-push-secret-boundaries.py
```

The official API generator must run in its own serialized Cargo checkpoint:

```sh
bun --filter @serverbee/web generate:api-types
```

Do not edit `apps/web/openapi.json` or `apps/web/src/lib/api-types.ts` by hand.
The dispatcher freezes source hashes, runs this command, checks unrelated hashes,
then returns control for inspection and commit.

Dispatcher heavy checks, after source/candidate freeze:

```sh
cargo test -p serverbee-server --lib service::push_envelope::tests --locked
cargo test -p serverbee-server --test mobile_push_integration --test router_mobile --locked
cargo test -p serverbee-server --lib service::apns::tests --locked
cargo clippy --workspace --benches --tests --examples --all-features --locked -- -D warnings
```

For the stitched trace, create an isolated temporary directory, set
`SERVERBEE_PUSH_TRACE_DIR` to it and run this **one** Server test. It exports only
synthetic fixture material, including its fixed non-secret content key:

```sh
SERVERBEE_PUSH_TRACE_DIR=/private/tmp/serverbee-t199-trace \
  cargo test -p serverbee-server --test mobile_push_integration \
  encrypted_test_uses_real_relay_admission_transport_and_legacy_migration --locked -- --exact
cd apps/ios
xcodegen generate
TEST_RUNNER_SERVERBEE_PUSH_TRACE_DIR=/private/tmp/serverbee-t199-trace \
  xcodebuild -project ServerBee.xcodeproj -scheme ServerBee \
  -destination "platform=iOS Simulator,id=$SIM_UDID" \
  -skipPackagePluginValidation \
  -resultBundlePath /private/tmp/serverbee-t199-ios.xcresult test
```

Run the iOS trace within 30 minutes of the Server trace; delivery/render expiry is authenticated
and intentionally enforced. Authenticated taps on previously displayed notifications retain their target beyond that window. Set `SIM_UDID` to the assigned dedicated Simulator, never a user device or `booted`. Use a fresh result-bundle path for each run. Verify
in `.xcresult` that both `testStitchedServerRelayPayloadThroughActualNotificationExtension`
and `testStitchedServerRelayCiphertextColdTapValidatesCurrentAccount` actually
executed without skips. Record total executed, failed and skipped counts. Without
the trace environment these two tests explicitly skip; a green suite with skips
does not establish the stitched path.

The trace test uses real Server HTTP login, mobile sessions, revisioned
subscriptions, migrated SQLite, content registration, recipient derivation and
Rust encryption, durable outbox admission and production delivery workers. It
starts the real stateless Relay request handler using synthetic APNs credentials;
only outbound APNs fetch is mocked. The production transport constructs payload,
headers and ES256 JWT. The captured payload then crosses the actual Swift NSE
`didReceive` boundary with only the shared Keychain read substituted. The app
consumes the same ciphertext through the early delegate buffer and authenticated
router. Rust and Swift additionally consume fixed interoperability vectors.
Workers-runtime tests separately exercise actual workerd request boundaries.
These tests do not prove live APNs acceptance, presentation or a human tap.

The late-response test replaces the APNs token/content-key ID over authenticated
Server HTTP, then releases an old terminal provider response. The replacement
registration remains usable; a terminal verdict for the current revision may
invalidate only that revision. Payload/configuration and transient errors retain
registration. There is no Relay grant rotation or attestation fixture.

The migration trace registers a real legacy token before verified setup and
checks the selector used by legacy APNs, later legacy re-entry, disable and
unregister, while retaining other users/installations. A held first legacy provider request additionally proves that a later cached
recipient is revalidated after migration before starting its send; already
in-flight requests cannot be retracted. Existing notification
service tests remain relevant for external-channel behavior.

## Isolated signing and Relay readiness

Use a separate Worker, TLS hostname, fixed topic and publisher-owned signing
key. Configure the Worker variables and `APNS_PRIVATE_KEY` secret described in
`apps/push-relay/README.md`; never install Apple keys on the Server or commit them.
Confirm both the topic and signed main app Bundle ID are `app.serverbee`; the
embedded extension is `app.serverbee.notifications`. Both app and extension must
share the signed Keychain group and provisioning team; the embedded extension
must include English and zh-Hans resources. Inspect effective signed entitlements,
not only `project.yml`. Debug uses sandbox APNs; distribution uses production.
No App Attest or Relay database is required. HTTPS is required before the app
transmits its content key to the authenticated Server. Server API and refresh
requests reject all redirects, including same-origin redirects; use final URLs.

The public endpoint accepts resource/quota abuse and known-token junk/replay
notifications. The fixed generic fallback may be shown when NSE fails or times
out. Preserve bounded streaming reads, limits, deadlines and concurrency; rate
maps are isolate-local best-effort protections, not a global cost guarantee.

The native secret-boundary script compiles only the production Foundation
transport and a synthetic macOS client. A temporary HTTPS origin returns 307/308
redirects to HTTP, a different HTTPS authority and the same origin. The client
uses test-only localhost certificate trust; production retains system TLS trust.
The script requires the original endpoint to receive each secret-bearing request
and every redirect target to receive zero connections. Its entitlement checks
inspect source configuration only. A sandbox denial of local socket binding is a
blocked runtime check, not a passing redirect test.

`PushKeychainIsolationTests` uses real Security.framework queries in the app-hosted
Simulator tests: access, refresh and revocation credentials stay in the
explicit app-private group; only the content-key record uses the shared group.
Refresh preserves that content key and paired-login scope; logout deletes it.
Debug and Release put the private group first, and the extension has only the
shared group. Missing or unexpanded group configuration fails closed. Simulator
queries establish Simulator storage behavior only. Physical isolation still
requires inspecting both effective signed entitlement sets and probing that the
actual extension cannot read the app-private credential service. Record static
configuration, Simulator queries and signed-device evidence separately.

For an in-place update of an existing `app.serverbee` installation, compare its
previous effective App ID prefix/private access group with the candidate, then
verify the server URL, installation ID, login and pending logout recovery remain
readable. The private service label stays `com.serverbee.mobile`; changing a
Bundle ID does not migrate a separately installed app or a different access
group. `testOfficialIdentityReadsHistoricalServiceInDefaultPrivateGroup` seeds
an item using that historical service and the implicit default group, then reads
it through the production explicit private-group query. This is Simulator
coverage only. If a push-enabled test build used the old shared group, repeat
notification setup and verify the new key/registration before testing delivery.

## Physical-device smoke record

Record candidate SHA, signed app/extension versions and entitlements, Relay
revision/configuration names (no values or tokens), environment, timestamps and
observed language. Keep three evidence columns: Server registration, provider
verdict, and device presentation/navigation.

1. Before opt-in, sign in and foreground/reconnect. Confirm no permission prompt.
2. Read the privacy disclosure, enable a category, allow system notifications
   and confirm authenticated Server registration.
3. Send a test with a second installation signed in. Only the initiating
   installation should receive it. Record APNs acceptance separately.
4. Observe one foreground system banner with no WebSocket banner. Repeat while
   backgrounded and terminated. Confirm localized title/body in English and
   Simplified Chinese. Tap a cold-launch test and confirm the current account
   opens after authentication restoration.
5. Change account/deployment or paired login before tapping an old notification.
   It must not navigate in the replacement context. Remove the local content
   key on an isolated build and confirm generic fallback, without plaintext
   event data. Restore through explicit setup retry.
6. Refresh authentication and rotate the APNs token. Confirm registration survives
   normal refresh and late provider errors cannot invalidate its replacement.
7. Disable notifications, logout, revoke the device, expire its mobile session,
   or change account credentials. A subsequent test must not deliver. Verify
   legacy register cannot restore plaintext for a migrated installation and
   unrelated legacy installations/external channels remain eligible.
8. Repeat development and distribution environments with correct topics. Test
   configuration/payload failures separately from an actual APNs `Unregistered`
   verdict. Do not turn every HTTP 400 into token deletion.

Mark every unrun live row as NOT RUN. A fixture, Simulator build, APNs receipt or
successful HTTP operation cannot stand in for observed background/terminated
presentation or tap navigation. Lost network responses may hide accepted sends;
durable retries do not promise exactly-once presentation.


## Durable outbox behavior checks (#200)

The `mobile_push_integration` suite runs duplicate HTTP enqueueing, a migrated
SQLite reopen followed by production worker startup, original-creation expiry,
retryable outages and rate limits, permanent provider errors, and revision-safe
late terminal responses. Queue admission returns `pending`; status is read from
`GET /api/mobile/push/test/{event_id}` under the current installation/session.
The stitched Server trace must run before iOS: it waits for the durable worker's
actual Relay/APNs fixture receipt before exporting the payload. The two named
Swift stitched tests above must still execute with zero skips.

Revocation tests change subscriptions, logout, device revocation, account deletion,
password reset and roles through real HTTP; only persisted expiry time is advanced
for the session/queue clock boundaries. Multiple-device and slow-network tests
exercise independent worker outcomes, 15-second network timeout and responsive
HTTP admission. Four workers and 30-second durable leases bound concurrent sends;
a restart may wait for an abandoned lease, never renew message expiry. Backoff
starts at 2 seconds and doubles up to 256 seconds. Terminal ciphertext is erased;
secret-free receipts remain as duplicate-admission tombstones.

Physical-device rows remain NOT RUN until separately observed. Add an isolated
Relay outage/restart exercise: verify pending/retryable status, restore the Relay
inside the 30-minute window, then record the provider receipt and actual phone
presentation separately. Repeat after disable/revocation and past expiry; already
in-flight or accepted notifications are not retractable.

The iOS recovery coordinator distinguishes an unknown reply, confirmed absence
and confirmed admission in app-private saved metadata. When registration changes,
an owned status lookup must establish absence before updating the same UUID's
expected revision. Existing admitted work is queried rather than resubmitted;
even a later 404 cannot recreate its original expiry window. Recovery tests cover
lost accepted replies, old saved metadata, restart, definite 404, conflicting late
admission and previous-account completions. `user_mutations_wait_for_outbox_writer_before_reading_revocation_guards`
holds a real SQLite outbox writer while authenticated user DELETE/password/role
operations start; each must wait successfully and prevent later eligible sends.

## Scheduled-task failure summaries (#203)

Automated Server coverage runs through the real scheduler, HTTP sessions,
subscriptions, Agent WebSocket reply path and migrated SQLite:

```sh
cargo test -p serverbee-server --test mobile_push_integration task_outcomes
cargo test -p serverbee-server --lib service::task_scheduler
```

The iOS XCTest bundle includes `TaskPushNavigationTests` for authenticated
exact-run taps, cold/warm taps after the delivery window, early delegate buffering, count-only rendering, stale account
responses and deleted/forbidden targets. Late taps must still issue an authenticated exact-run Server read; a current 403 shows the dismissible fallback. `EncryptedPushNavigationTests` also verifies the shared test-category late-tap policy. These fixtures substitute Agent command
execution, Apple and network boundaries; they do not establish native delivery.

With a signed app and isolated configured Relay, subscribe two installations as
the task owner and a third as another administrator. Manually run a scheduled
task created by the other administrator and verify that only the initiator's
installations receive the final summary. For an automatic run, only the creator
should receive it. Hold one target while others fail; observe no summary before
its final reply. Check default retry-success silence, opted-in final retry success, exhausted failure, command timeout,
scheduler deadline, offline and capability-denial counts, including all-denied
runs. Confirm that the banner contains no task name, command or output.

Record APNs provider acceptance, foreground/background/terminated presentation
and exact-run authenticated navigation separately. Tap after deleting the task,
revoking administrator access, switching accounts and during cold launch; verify
safe fallback or rejection. Queued delivery must stop after current role or
subscription revocation. Successful-run delivery is disabled by default. Enable **Successful task runs** only on one owner installation and confirm one final success summary for that installation, with no command or output. Disable **Final task failures** on it to verify the success option is independent. A mixed-target failure must follow only the failure subscription. Opt out while success is queued, or demote the owner before dispatch; neither may send. A failed save must retain the confirmed preference and display the unconfirmed choice and error. Run these cases after restart as well, preserving the original run identity and deadline.

The task recovery cases inject a real SQLite INSERT failure after actual scheduler
final attempts, preserve a separately committed drained summary, and restore the
production worker after failure and a migrated SQLite snapshot reopen. They assert
original run UUID/time/expiry, independent per-installation receipts, repeated
recovery without duplicate admission, and current role/subscription/session gates.
The restart snapshot also includes a real running task with an intermediate
failure waiting for its next retry: startup must mark it incomplete and never
infer a final summary from that row. A separate SQLite fault on the drain-proof
write verifies the scheduler retains its run guard and retries with its original
time. Advancing only persisted expiry timestamps tests the clock boundary; no
internal policy or scheduler boundary is substituted. A process interrupted
before a drain proof commits remains incomplete, even if it already has results.

For signed-device acceptance, deliver both an opted-in task success and a task failure, wait more than 30 minutes,
and tap it in both a warm and a terminated app. Verify the exact run opens with
current Server authorization. Repeat after access revocation, task deletion and
account replacement, checking fallback and isolation. Record these rows as NOT RUN
until separately observed; encrypted fixture navigation does not prove native taps.


## Alert subscription acceptance (#201)

Run the Server `mobile_push_integration` tests whose names start with `alert_`,
plus `event_alerts_enqueue_general_category_but_security_matches_do_not`.
They use HTTP-created rules, subscriptions and authenticated sessions, migrated
SQLite, production evaluation, and the production outbox worker. Only the
external Relay HTTP boundary is substituted. The shared fixed alert-envelope
vector is checked by Rust and Swift; `AlertPushNavigationTests` uses the actual
notification extension for live-time trigger/recovery rendering and the early
delegate/router path for cold taps. Run iOS tests once in English and once with
`-AppleLanguages (zh-Hans)` to observe both localized titles and bodies.

On the isolated signed real device, enable **Alerts and recoveries**, then
verify a trigger and recovery without a notification group in foreground,
background and terminated states. Record provider acceptance, observed banner,
and authenticated alert-detail navigation separately. Verify old-account and
old-deployment taps cannot navigate; delete the rule or rearm the alert before
tapping an old cycle and verify **Alert not found** / **Back to alerts**. Unsubscribe
before dispatch and confirm that installation receives no later eligible send;
a second subscribed installation must still receive its own event. Verify
maintenance, disabled rules and repeat suppression against external-channel
behavior. Live APNs and signed-device presentation remain separate live evidence.


The rollback/restart tests inject a second-recipient INSERT failure and a deferred
SQLite COMMIT failure for trigger, recovery and rearm. They verify all recipient
jobs, state and cache roll back together, then reopen the database and successfully
admit one logical event without changing its committed creation/expiry on retry.
`alert_delivery_expiry_preserves_current_authenticated_detail_lookup` separately
checks the production worker's expiry, current HTTP authorization and deleted-target
response. Run these cases and both shared-category migration tests on the frozen SHA.

For receiving, `AlertPushNavigationTests` covers cold taps after one hour and warm
taps after seven days through the production renderer, delegate and router. It also
checks normal delivery/NSE expiry, tampering, login/deployment/installation isolation,
bounded ciphertext and current detail fetch with 403/404 fallback. On the signed
isolated device, leave an already-presented alert for more than 30 minutes before
warm and cold taps. Its exact detail must still resolve or show the safe fallback;
an expired notification arriving for the first time must not render alert content.
These are separate from provider acceptance and still require live device evidence.


## One-shot event durability (#201 correction)

Migration `m20261003_000084_alert_event_intents` is reserved for this ticket.
`ws_ip_event_intents_recover_insert_and_commit_failures_on_startup` sends real
IpChanged and SystemInfo frames, fails second-recipient INSERT or actual COMMIT,
and reopens migrated file SQLite. The production startup evaluator must recover
without another IP transition, retaining the original cycle, event time and
30-minute deadline. A second event pending admission must respect once suppression.
`ws_ip_event_replay_rechecks_installation_eligibility` disables an installation
before replay. `ws_ip_intent_capture_failure_does_not_consume_source_update`
verifies failed durable capture rolls back source addresses and closes the socket;
reconnection with the same current SystemInfo IP recovers the unconsumed change.
`ws_event_replay_does_not_restart_an_expired_mobile_deadline` checks mobile expiry
independently of the existing best-effort external group. Captured intents contain
only pending event metadata and are removed atomically with alert state/jobs.
External group dispatch follows that successful commit and is not repeated by
subsequent event-job retries or normal startup replay. Provider acceptance and
real signed-device presentation remain separate evidence.

### Capability source recovery

- On an enrolled Agent, grant a high-risk temporary capability after enabling a `capability_grant_detected` rule. With intent INSERT or deferred COMMIT failing, confirm no admission Ack, alert state, outbox, or capability-change audit is committed. Remove the fault and reconnect/restart the Agent and Server without a second grant. The original journal event must be admitted once, preserve its original time and 30-minute mobile deadline, and reach each currently eligible installation and existing external group once.
- Revoke the capability before the original event is acknowledged. Replay must report the current authority snapshot and never restore the revoked permission. Low-risk Granted, Expired and Revoked remain audited without new alerts. Once-mode suppression applies to distinct later grants. Replay after 30 minutes must skip mobile delivery without extending expiry; external channels retain their existing policy.
- A Server advertises `capability_event_ack` in Welcome and acknowledges only frames carrying the original `occurred_at`; legacy frames keep their existing response behavior. Older Servers retain the existing fire-and-forget transition behavior, which consumes journal entries after an attempted send and discards older unsent entries on connect to avoid duplicate legacy alerts after a peer upgrade; modern source recovery requires both updated Agent and Server. The Agent retains original events across reconnect/restart, removes them only after Ack on its current owned socket, and discards prior-destination events when the confirmed deployment/enrolled Server changes. Existing grants at first journal initialization are not reconstructed as new grants. A new journal whose initial write fails remains pending while reporting and locally authorized commands continue. An existing readable journal is not rewritten during startup. An unreadable or corrupt existing journal still rejects startup without overwriting retained events; storage must be repaired before it can recover. Later write failures keep original events and observed times in memory, apply current local authority immediately, and retry persistence. Events that have never reached durable storage cannot survive process loss during that storage outage.

- With a real Reporter and live PTY, block journal temporary-file creation, then revoke terminal while a source retry is pending. Also fail an owned Ack deletion before forcing reconnect. `reporter_dirty_journal_retry_reaps_revoked_pty_and_recovers_original_event` and `reporter_ack_write_failure_keeps_source_and_reaps_pty_before_reconnect` drive real grant-file ticks, WebSockets and shells that ignore HUP. These three fixtures select `/bin/sh` on their own Reporter instance without changing process-wide `SHELL`; the shell-published PID must match the actual top-level PTY child and be a live waitable child of the test process. The old PID must disappear and already be reaped by production; restoring storage must replay the same source UUID/time using current revoked authority. `reporter_startup_write_failure_preserves_duties_and_cancellation_reaps_pty` verifies initial write failure does not stop reporting or authorized terminal commands, and cancellation cleans the real child while the server socket remains open. Journal errors do not consume a source or abandon connection resources; runtime Drop invokes the same explicit shutdown on every exit path.


## Combined alert and task integration checkpoint

`task_outcomes::alerts_and_final_tasks_share_queue_without_crossing_subscription_gates`
queues an alert and a real final scheduler outcome in the same migrated SQLite
store. It verifies independent installation subscriptions, failure and success
categories, original deadlines and single-recipient dispatch after an HTTP opt-out.
It also represents an already queued task upgraded from the accepted task schema,
where the newly added category column defaults to `test`; the retained task target
must still require current ownership, role and outcome-specific preference.
`shared_category_and_task_migrations_preserve_both_integration_orders` runs the
actual category and task migrations in both orders and preserves ciphertext,
revision, task targets and all existing preference columns.
`TaskPushNavigationTests.testTaskCategoryRejectsMixedAlertTargetsForDeliveryAndLateTaps`
rejects an authenticated task payload containing a second valid alert target.
Run these checks on the final officially generated combined SHA. Source parsing
and parent check results do not establish their execution or signed-device delivery.
