# Encrypted current-installation notification verification

This checklist covers #199 encrypted delivery and #200 durable test retries.
Alert/security/task event selection belongs to later tickets. Never use Heeler's Relay, production
credentials, a user's existing Simulator, or a live account for fixtures.

## Local stitched path

Prerequisites: installed repository Bun dependencies, OpenSSL 3, Rust toolchain,
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
  -destination 'platform=iOS Simulator,id=979DE414-28A0-4D7F-8F7F-A90E630F9B5D' \
  -skipPackagePluginValidation \
  -resultBundlePath /private/tmp/serverbee-t199-ios.xcresult test
```

Run the iOS trace within 30 minutes of the Server trace; expiry is authenticated
and intentionally enforced. Use a fresh result-bundle path for each run. Verify
in `.xcresult` that both `testStitchedServerRelayPayloadThroughActualNotificationExtension`
and `testStitchedServerRelayCiphertextColdTapValidatesCurrentAccount` actually
executed without skips. Record total executed, failed and skipped counts. Without
the trace environment these two tests explicitly skip; a green suite with skips
does not establish the stitched path.

The trace test uses real Server HTTP login, mobile sessions, revisioned
subscriptions, migrated SQLite, content registration, recipient derivation and
Rust encryption, durable outbox admission and production delivery workers. It starts the real Bun Relay handler/database and obtains a
grant through actual certificate/nonce/key verification. Only Apple's attestation
material and outbound APNs HTTP/2 provider are fixtures. The real APNs transport
constructs the payload, headers and cached ES256 JWT. The captured provider payload
then crosses the actual Swift extension `didReceive` boundary, with only the
shared Keychain read substituted. The app consumes the same ciphertext through
the early AppDelegate buffer and authenticated router. Rust and Swift additionally
consume one fixed interoperability vector. No internal policy or SQLite service
is replaced. These tests do not prove Apple's genuine App Attest, HTTP/2 provider
acceptance, native notification presentation or a human tap.

The separate late-response test renews the real Relay grant through a signed
assertion, replaces the APNs token/content-key ID over authenticated Server HTTP,
then releases an old terminal provider response. Current registration must remain
usable; a terminal verdict for the current revision must invalidate only that
revision. Configuration/payload errors and transient failures retain registration.
The migration trace registers a real legacy token before verified setup and
checks the selector used by legacy APNs, later legacy re-entry, disable and
unregister, while retaining other users/installations. A held first legacy provider request additionally proves that a later cached
recipient is revalidated after migration before starting its send; already
in-flight requests cannot be retracted. Existing notification
service tests remain relevant for external-channel behavior.

## Isolated signing and Relay readiness

Use a separate Relay database, TLS hostname, topic and publisher-owned signing
key. Configure the names in `apps/push-relay/README.md`; never install Apple keys
on a self-hosted Server or commit them. Protect the private key file and SQLite
directory. Audit the Apple root pin and approved app versions independently.
Confirm the app topic matches the signed application's bundle ID. Both app and
extension must share the signed Keychain group and provisioning team; confirm the
extension is embedded in the app and contains English and zh-Hans resources.
Inspect the effective signed entitlements, not just `project.yml`. Debug must
pair development App Attest with sandbox APNs; distribution must pair production
attestation with production APNs. Genuine admission fails closed on unsupported
or failed App Attest, without a fallback device grant. HTTPS is required for the
app's Server connection before transmitting the content key. Server API and
refresh requests reject every redirect, including same-origin redirects; configure
the final Server URL rather than relying on a reverse-proxy redirect.

The native secret-boundary script compiles only the production Foundation
transport and a synthetic macOS client. A temporary HTTPS origin returns 307/308
redirects to HTTP, a different HTTPS authority and the same origin. The client
uses test-only localhost certificate trust; production retains system TLS trust.
The script requires the original endpoint to receive each secret-bearing request
and every redirect target to receive zero connections. Its entitlement checks
inspect source configuration only. A sandbox denial of local socket binding is a
blocked runtime check, not a passing redirect test.

`PushKeychainIsolationTests` uses real Security.framework queries in the app-hosted
Simulator tests: access, refresh, revocation and Relay credentials stay in the
explicit app-private group; only the content-key record uses the shared group.
Refresh preserves that content key and paired-login scope; logout deletes it.
Debug and Release put the private group first, and the extension has only the
shared group. Missing or unexpanded group configuration fails closed. Simulator
queries establish Simulator storage behavior only. Physical isolation still
requires inspecting both effective signed entitlement sets and probing that the
actual extension cannot read the app-private credential service. Record static
configuration, Simulator queries and signed-device evidence separately.

## Physical-device smoke record

Record candidate SHA, signed app/extension versions and entitlements, Relay
revision/configuration names (no values or tokens), environment, timestamps and
observed language. Keep three evidence columns: genuine admission, provider
verdict, and device presentation/navigation.

1. Before opt-in, sign in and foreground/reconnect. Confirm no permission prompt.
2. Read the privacy disclosure, enable a category, allow system notifications
   and confirm genuine App Attest admission and Server registration.
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
behavior. Genuine App Attest and APNs presentation remain separate live evidence.


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
