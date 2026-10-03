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

## Scheduled-task failure summaries (#203)

Automated Server coverage runs through the real scheduler, HTTP sessions,
subscriptions, Agent WebSocket reply path and migrated SQLite:

```sh
cargo test -p serverbee-server --test mobile_push_integration task_outcomes
cargo test -p serverbee-server --lib service::task_scheduler
```

The iOS XCTest bundle includes `TaskPushNavigationTests` for authenticated
exact-run taps, early delegate buffering, count-only rendering, stale account
responses and deleted/forbidden targets. These fixtures substitute Agent command
execution, Apple and network boundaries; they do not establish native delivery.

With a signed app and isolated configured Relay, subscribe two installations as
the task owner and a third as another administrator. Manually run a scheduled
task created by the other administrator and verify that only the initiator's
installations receive the final summary. For an automatic run, only the creator
should receive it. Hold one target while others fail; observe no summary before
its final reply. Check retry-success silence, exhausted failure, command timeout,
scheduler deadline, offline and capability-denial counts, including all-denied
runs. Confirm that the banner contains no task name, command or output.

Record APNs provider acceptance, foreground/background/terminated presentation
and exact-run authenticated navigation separately. Tap after deleting the task,
revoking administrator access, switching accounts and during cold launch; verify
safe fallback or rejection. Queued delivery must stop after current role or
subscription revocation. Successful-run notification delivery belongs to #204.
