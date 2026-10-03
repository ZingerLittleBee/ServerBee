> Revision 2026-10-03: this unreleased specification supersedes the original App Attest/grant design with public stateless Cloudflare Workers forwarding. The accepted tradeoff includes quota abuse and generic-notification harassment. Server ownership, shared-key AEAD, mobile-session security and durable business delivery remain required.

## Problem Statement

ServerBee users need timely iPhone notifications when monitored Servers raise or recover from alerts, security rules match, or scheduled tasks finish with problems. Browser and WebSocket updates are insufficient while the iOS app is backgrounded or not running.

ServerBee already has partial APNs support, but its current lifecycle can lose device registration during authentication refresh. Sending also lacks category and recipient selection, per-device APNs environment handling, and correct notification-target identity. Existing code and local tests do not establish working background or terminated-app delivery.

Users of the official iOS app must not need their own Apple Developer credentials or a separately signed app. Self-hosted Server operators must not receive the publisher's APNs private key. Notification content should remain private from the shared delivery infrastructure.

## Solution

Deliver Server-originated mobile notifications through this pipeline:

**ServerBee Server -> dedicated ServerBee Push Relay -> APNs -> iOS.**

Users explicitly enable Mobile notification subscriptions by category in iOS Settings. The Server selects eligible recipients, encrypts each notification for its installation, and persists pending delivery for up to 30 minutes. The public stateless Cloudflare Worker validates bounded requests and forwards encrypted content using the official app's APNs credentials. An iOS Notification Service Extension decrypts the content before presentation.

Alerts include trigger and recovery notifications. Security notifications follow existing security alert rules and are restricted to administrators. Scheduled tasks, including manually started runs, notify the appropriate owner once per completed run, with failures enabled by subscription and successful results separately opt-in. Notification taps open the relevant authenticated detail rather than an unrelated screen.

## User Stories

1. As an iOS user, I want alert notifications while the app is backgrounded, so that I can react without keeping the dashboard open.
2. As an iOS user, I want notifications when the app is not running, so that closing it does not stop monitoring notifications.
3. As an official-app user, I want notifications without provisioning Apple certificates, so that enabling them does not require an Apple Developer account.
4. As a self-hosted Server operator, I want my Server to originate notifications, so that local event and account policies remain authoritative.
5. As a user, I want a clear explanation of the relay and privacy boundary before granting permission, so that I understand where delivery metadata goes.
6. As a user, I want to enable notifications explicitly in iOS Settings, so that signing in does not unexpectedly prompt me for notification permission.
7. As a user, I want separate subscriptions for alerts, security, and task outcomes, so that I receive the categories relevant to me.
8. As a user, I want the app to show preferences confirmed by the Server, so that a failed save is not presented as successful.
9. As a user, I want visible permission and registration status, so that I can distinguish disabled permission from a delivery setup failure.
10. As a user, I want to retry registration and open system notification settings, so that I can repair setup without reinstalling the app.
11. As a user, I want a test notification to my current installation, so that I can verify my own setup without notifying other users.
12. As a subscribed user, I want alert-trigger notifications, so that I know when an enabled monitoring rule requires attention.
13. As a subscribed user, I want recovery notifications, so that I know when a previously reported alert returns to normal.
14. As a user, I want alerts to respect existing maintenance and suppression policies, so that planned work does not generate unwanted mobile notifications.
15. As a user, I want Mobile notification subscriptions to work without an APNs channel or notification group, so that setup is possible entirely from iOS.
16. As an administrator, I want security notifications to follow enabled security rules, thresholds, exclusions, and deduplication, so that routine raw detection events do not flood my phone.
17. As an administrator, I want notifications for admitted SSH new-IP login, brute-force, and port-scan rule matches, so that I can respond to relevant security findings.
18. As a member, I want notification delivery to respect my current role, so that subscribing cannot expose administrator-only security or task information.
19. As a manual task initiator, I want the final outcome of my scheduled-task run, so that I know whether the operation I requested completed.
20. As an automatic task's creator, I want notifications for its final failures, so that unattended executions can receive attention.
21. As a task recipient, I want success notifications to be optional, so that routine successful runs do not interrupt me by default.
22. As a task recipient, I want one summary after all target Servers finish, so that a multi-Server run does not generate a notification per machine.
23. As a task recipient, I want intermediate retries to remain silent, so that transient failures are not confused with the final outcome.
24. As a task recipient, I want the final summary to identify failures, timeouts, offline targets, and execution denials, so that I can assess the run accurately.
25. As a task recipient, I want commands and command output excluded from push content, so that potentially sensitive execution details remain behind authentication.
26. As a user, I want to tap an alert notification and open the corresponding alert, so that I can inspect its actual history and state.
27. As an administrator, I want to tap a security notification and open the relevant Server security detail, so that I can investigate its context.
28. As a task recipient, I want to tap a task notification and open that run's results, so that I do not have to locate the run manually.
29. As a user opening a notification from a terminated app, I want its target retained until login restoration completes, so that startup timing does not lose the tap.
30. As a user, I want unavailable or deleted notification targets to fall back safely, so that an old notification does not strand me on a broken detail screen.
31. As a user who changed accounts or deployments, I want old notifications prevented from navigating within my current account, so that identities cannot be confused.
32. As a foreground user, I want one system notification banner, so that APNs and WebSocket updates do not create duplicate banners for the same event.
33. As a signed-in user, I want routine authentication refresh to preserve notification registration, so that receiving notifications does not depend on relaunching the app.
34. As a user, I want permission and registration reconciled after returning to the app or recovering connectivity, so that transient setup failures can recover.
35. As a user, I want registration bound to my installation and authenticated account, so that another user cannot overwrite or unregister it by guessing its identifier.
36. As a user signing out, I want subsequent delivery for that account stopped, so that account notifications do not continue after logout.
37. As an administrator revoking a mobile device or account credentials, I want queued delivery to respect revocation, so that previously queued messages do not bypass the new policy.
38. As a user, I want expired sessions and removed roles to stop eligible delivery, so that an old registration does not retain access indefinitely.
39. As a user with multiple devices, I want each subscribed installation evaluated independently, so that success or failure for one device does not suppress another's delivery.
40. As a development or distribution user, I want my device's APNs environment handled correctly, so that sandbox and production devices can coexist.
41. As a user, I want notification content encrypted between my Server and installation, so that the relay cannot read Server names, IP addresses, alert messages, or task summaries.
42. As a user, I want a generic notification fallback when decryption cannot complete, so that failures do not expose plaintext through another path.
43. As an app publisher, I want APNs signing credentials retained only by the relay, so that public Server distribution does not disclose them.
44. As a relay operator, I want a small public stateless forwarding service with explicit resource bounds, accepting quota abuse and known-token generic-notification risks without building another authorization system.
45. As a user, I want failed authenticated registration or unavailable delivery to show an honest retryable state while monitoring and login remain usable.
46. As a Server operator, I want pending deliveries to survive restart and temporary relay outages, so that accepted local work is not discarded immediately.
47. As a user, I want notifications older than 30 minutes discarded, so that an outage does not end with a flood of stale messages.
48. As a Server operator, I want failures classified as accepted, retryable, permanent, or expired, so that delivery problems can be diagnosed accurately.
49. As a user whose device token changes, I want stale send responses unable to delete its replacement, so that recovery cannot be undone by a late response.
50. As a user, I want an APNs configuration or payload error distinguished from an invalid device token, so that valid registration is not erased by an unrelated provider error.
51. As an existing notification-channel user, I want external channels to retain their configured behavior, so that mobile delivery does not alter Webhook, Telegram, Bark, or email workflows.
52. As a user migrating to relay delivery, I want an event delivered through one mobile path, so that the legacy APNs channel and automatic delivery do not both notify the same new registration.
53. As an English or Simplified Chinese user, I want localized settings, status, and notification text, so that the feature is understandable in my selected language.
54. As a maintainer, I want automated contract tests and separate real-device evidence, so that local test success is not mistaken for verified APNs presentation.

## Implementation Decisions

- **Architecture and ownership:** introduce a dedicated ServerBee Push Relay, separate from Heeler's deployed relay. ServerBee Server owns event selection, subscriptions, user ownership, and pending delivery. The relay owns APNs credential custody and bounded forwarding. Apple credentials are never distributed in the app or self-hosted Server. Refer to the mobile-notification relay ADR; Agent Authority and enrollment behavior are unaffected.
- **Reuse existing behavior:** extend the existing mobile registration, authentication, alert evaluation, security rule evaluation, task-run aggregation, iOS notification manager, and navigation responsibilities. Heeler is reference material for encryption, early delegate setup, APNs transport, and privacy disclosure; adopt public stateless forwarding, while retaining ServerBee authenticated HTTPS registration instead of SSH ownership.
- **Subscription API:** extend the authenticated mobile registration family with current-installation subscription/status operations and a targeted test operation. Derive user and installation ownership from the authenticated mobile context. Reject cross-user changes and unauthorized categories. Return confirmed preferences and actionable registration status, expose OpenAPI contracts, and update generated client contracts as needed.
- **Registration data:** persist the stable installation identity, authenticated user/session association, APNs token and environment, notification key identity, category preferences, optional task-success preference, and registration revision. Token or key replacement must be versioned. Preserve existing ownership protections through forward-only migrations.
- **Recipient policy:** alerts go to eligible subscribed users. Security rule notifications require a current administrator role. Scheduled-task outcomes go to the manual initiator or, for automatic runs, the creator, subject to current task access. Evaluate each installation independently and revalidate user existence, role, valid mobile session, subscription, and registration revision before sending.
- **Event gates:** alert trigger/recovery and security notifications reuse existing rule filtering, maintenance handling, and notification suppression. Mobile enqueueing must not require a notification group. A security-rule match generates the security category once rather than a second general-alert delivery. Generate stable logical event identities for deduplication.
- **Task boundaries:** notify only after every target of a scheduled-task run reaches its final outcome. Aggregate final results by run, excluding superseded retry failures. A capability denial, offline target, exhausted failure, or final timeout contributes to a failed summary. Successful summaries require explicit opt-in. Link to the same run identity; exclude commands and output from notification content.
- **Authentication lifecycle:** atomically preserve and rebind valid installation registration during ordinary credential refresh. Logout, device revocation, password changes, user deletion, and mobile-session expiry stop future eligible sends, including queued messages. In-flight operations must use captured identity/revision and reject stale completion. Revocation cannot retract notifications already accepted by APNs.
- **Content encryption:** use a versioned AES-256-GCM envelope with per-installation content keys, authenticated metadata, fresh nonces, and cross-language interoperability vectors. Bind deployment, account, category, event identity, and navigation target inside encrypted content. The iOS copy resides in shared Keychain storage accessible to the Notification Service Extension; the Server copy is exchanged only through authenticated secure registration. No plaintext fallback or notification content keys go to the relay.
- **Public Relay boundary:** one `POST /v1/send` accepts `device_token`, `environment`, `event_id`, `expires_at` and the unchanged encrypted `envelope`. No App Attest, version/distribution policy, challenge/grant lifecycle, Relay database or replacement authorization. Anyone can consume resources; knowing a valid token may enable junk/replay/generic-notification harassment. This accepted risk does not weaken Server ownership or authenticated tap checks.
- **Relay resources and transport:** Cloudflare Workers WebCrypto JWT + fetch, fixed APNs hosts/topic/environment and generic fallback. Enforce streamed byte and read-time limits before parsing, strict schema/size bounds, bounded concurrency and provider timeout/response reads. Hard-capacity per-source/per-target maps and isolate-wide limits are best effort, reset on isolate lifecycle and are not a global quota. No business plaintext or content keys enter Relay.
- **Durable delivery:** persist encrypted pending messages per logical event and eligible installation, unique against duplicate enqueueing, with expiry 30 minutes after the event. Resume eligible messages after restart; retry transient network failures, rate limits, and temporary provider errors with bounded backoff. Discard expired or newly ineligible work. Keep accepted, retryable, permanent, and expired outcomes distinct. Ambiguous network failures do not permit a claim of exactly-once presentation.
- **Safe cleanup:** delete or invalidate registration only for a classified terminal device condition and only if the sending revision still matches. Do not interpret every HTTP 400 response as an invalid token. Configuration, payload, authorization, and environment failures require their own recovery paths.
- **iOS experience:** retain one active deployment/account. Add category settings, task-success opt-in, permission and registration status, retry, a targeted test, and system-settings access. Request system permission only after explicit enablement. Reconcile registration on launch, foreground entry, and connectivity recovery. Failed saves remain visible and do not falsely update confirmed preferences.
- **Receiving and navigation:** install the notification delegate early and buffer callbacks/taps until dependencies and authentication are ready. Decrypt in the Notification Service Extension, falling back to generic text on failure. In the foreground, show the system banner without another WebSocket banner. Route alerts by complete alert identity, security to relevant Server security detail, and tasks by run identity. Reject deployment/account mismatches and unauthorized targets; safely fall back for deleted or unavailable resources.
- **Compatibility:** retain legitimate existing external-channel behavior. Explicitly separate relay registrations from legacy direct-APNs delivery so the same event cannot fan out through both paths to a newly registered installation. Document legacy migration behavior; do not silently send legacy plaintext through the official encrypted relay or require existing notification groups for the new subscriptions.
- **Privacy and documentation:** explain before permission is requested that the relay observes tokens, source IPs, timing, request size, environment, event/delivery metadata, and ciphertext, while lacking content keys. Update English and Chinese mobile and configuration documentation together, and keep user-facing iOS strings localized.
- **Dependencies and deployment:** prefer existing or established small implementations for cryptography, CBOR, and certificate validation rather than handwritten cryptographic primitives. Earlier library names were candidates, not separately approved dependency additions; follow repository dependency rules when selecting them. Provide isolated relay configuration, signing requirements, secret names, and a deployment runbook. A configured relay and signed app are prerequisites for real-device validation, not something proven by local compilation.

## Testing Decisions

- **Confirmed primary seam:** test the feature through the existing Server HTTP/integration harness and real event/lifecycle entry points. Use real migrated SQLite, real authentication and ownership checks, real rule evaluation, real task-run aggregation, and real outbox behavior. Substitute the external Relay network boundary rather than mocking internal policy modules or repository persistence.
- **Test external behavior:** assert which installation receives which logical event, whether a revoked or unsubscribed recipient stops receiving, what the API reports, what survives restart, and which detail a tap opens. Avoid tests coupled to private helper calls, internal query layout, or incidental task scheduling.
- **Server prior art:** extend the existing mobile-auth HTTP integration suite and push cross-user ownership tests. Follow existing security-rule tests for exclusions, maintenance, and deduplication, and existing scheduler tests for retry and final-result behavior. Keep existing authorization regressions covered.
- **Server acceptance matrix:** include registration before and after access-token refresh; logout/device/password/user revocation; session expiry and role change; member versus administrator categories; manual initiator versus automatic creator; multiple devices; failed preference updates; duplicate enqueueing; restart recovery; 30-minute expiry; transient versus permanent relay errors; stale-token cleanup races; and no dependency on a notification group.
- **Task acceptance matrix:** cover single and multiple target Servers, failure followed by successful retry, exhausted failures, offline targets, capability denials, timeouts, successful-run opt-in, unauthorized recipients, and exactly one logical summary at the final-run boundary. Do not treat an intermediate result or incomplete run as a completed successful run.
- **Relay boundary:** actual Workers runtime/request tests replace only external APNs networking. Verify streamed body cancellation and hard limits, slow body deadlines, bounded limiter cardinality, request concurrency, both APNs environments, fixed payload/headers, JWT caching and accepted/permanent/retryable provider reasons. Keep real Server encryption → Relay payload → Swift NSE fixtures and Wrangler deployment dry-run evidence separate from live APNs.
- **Encryption interoperability:** share fixed vectors across Rust and Swift. Verify valid decrypt/render/target behavior, tampering, wrong key, unsupported version, identity binding, and size limits. Include assertions that relay requests and queued message content do not contain plaintext notification data or content keys.
- **iOS boundary:** extend existing push-router, deep-link navigation, and logout-order tests. Use thin substitutes only for system permission/token callbacks, Keychain test isolation and authenticated HTTP boundaries where native services cannot run in unit tests. Cover early callbacks, cold-launch buffering, registration recovery, stale identity/generation, preferences, decryption, foreground presentation, and safe navigation fallback. Heeler's registration and encrypted-envelope tests are reference patterns.
- **Tooling checks:** regenerate the iOS project after new targets or sources; run relevant iOS builds/tests, Server integration suites, changed-crate formatting and required Clippy checks, generated API/client checks, localization review, and documentation contract checks using repository entry points. Report executed test counts and relevant failures.
- **Separate real-device acceptance:** using an isolated configured relay and correctly signed app, verify development/distribution APNs environment handling, encrypted APNs notifications in foreground/background/terminated states, cold-launch taps, token refresh, logout/revocation, and category changes. Record provider acceptance separately from observed presentation and navigation. Simulator tests, stubbed APNs responses, and a successful build do not satisfy this layer.

## Out of Scope

- Interactive terminal/one-shot command-result notifications, Agent upgrade results, and manual Incident CRUD notifications.
- Raw security-event fan-out that bypasses existing security alert rules.
- Multi-account or multi-deployment storage and automatic account switching from a notification.
- Critical-alert entitlements, Live Activities, notification action buttons, or a second WebSocket-driven notification banner.
- A public unauthenticated production relay, unsupported-device security downgrade, or distribution of the official app's APNs private key.
- Requiring users to sign their own app or provision their own Apple Developer credentials for the official-app workflow.
- Replacing existing external notification channels, redesigning their secret storage, or unrelated monitoring/authentication refactors.
- Guaranteed APNs presentation, exactly-once phone display, or replaying notifications older than 30 minutes.
- Modifying or deploying Heeler's existing relay.
- Deployment, Apple account mutations, secret installation, production configuration changes, code pushes, releases, or PR creation as an automatic consequence of this specification.

## Further Notes

- These requirements synthesize the user's selected design decisions. The user also confirmed the testing seams: Server integration behavior as the primary seam, separate iOS and Relay boundary checks, and distinct real-device acceptance.
- Current shortcomings were established by read-only source investigation of ServerBee and Heeler. No feature code, build, automated test run, or real-device APNs verification has been completed for this specification.
- Publishing this specification authorizes issue creation and the requested triage label. It does not by itself authorize deploying infrastructure or installing credentials.
- Keep implementation readiness distinct from live validation readiness. Missing relay deployment, Apple configuration, or signed-device access must be reported explicitly, with completed local checks retained as their own evidence.
- Official protocol references: [APNs registration](https://developer.apple.com/documentation/usernotifications/registering-your-app-with-apns), [APNs signing keys](https://developer.apple.com/help/account/capabilities/communicate-with-apns-using-authentication-tokens/), [notification content modification](https://developer.apple.com/documentation/usernotifications/modifying-content-in-newly-delivered-notifications), and [Workers WebCrypto](https://developers.cloudflare.com/workers/runtime-apis/web-crypto/).
