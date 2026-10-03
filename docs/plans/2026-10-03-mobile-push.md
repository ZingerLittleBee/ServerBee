# Server-originated mobile notifications

Status: design decisions agreed individually; awaiting final design confirmation.

## Outcome

Support notifications while the official ServerBee iOS app is foregrounded,
backgrounded, or not running:

`ServerBee Server -> ServerBee Push Relay -> APNs -> iOS`

The Server owns event selection, subscriptions, recipients, and pending
deliveries. The relay owns Apple credentials and validates device-bound
delivery authorization. It cannot decrypt notification content.

## Current implementation and confirmed gaps

- iOS already has permission handling, an APNs token callback, registration
  and logout calls, notification-tap routing, and APNs entitlements.
- Server already has mobile token registration and a direct APNs channel.
- Mobile authentication refresh deletes the old device registration without
  reliably restoring it during an already authenticated app session.
- APNs sending currently scans every registered token, ignores mobile
  session expiry, uses one environment for all devices, and treats all
  HTTP 400 responses as invalid tokens.
- A notification carrying both `server_id` and `rule_id` opens Server
  details; a rule-only fallback is not the actual alert-detail identity.
- Delegate callbacks can arrive before the app injects its push stores.
- The current relay reference is Heeler's separate-topic, encrypted,
  developer-operated Cloudflare Worker. Its public admission and SSH device
  registration are unsuitable for ServerBee's selected ownership model.

These are source findings, not runtime or real-device verification.

## Event and recipient policy

| Category | Event gate | Recipient | Tap target |
| --- | --- | --- | --- |
| Alerts | Existing enabled rules, trigger/recovery transitions, maintenance and notification suppression | Users subscribed to alerts | Alert detail using its complete identity |
| Security | Existing security-rule matching, filters, maintenance and deduplication | Subscribed administrators | Related Server security detail |
| Task failure | All target Servers finish their final attempts, with any failure, timeout, offline target, or capability denial | Manual initiator, or creator for an automatic run | Task results filtered to that run |
| Task success | Same final-run boundary, with success notifications enabled | Same task owner policy | Same run results |

Task retry attempts do not generate separate notifications. A multi-Server
run produces one logical summary. Commands and command output are excluded
from push content. The task scope is scheduled tasks, including manual runs
of those tasks; interactive execution and Agent upgrade results are outside
this first release.

Mobile delivery is independent of notification groups. Existing external
channels retain their group configuration. The automatic mobile path must
not also send the same event through a legacy APNs channel to a newly
registered relay installation.

## iOS ownership and interaction

- Keep the existing single active deployment/account model.
- Add a Notifications screen with category subscriptions, optional task
  success notifications, authorization/registration state, retry, test, and
  an entry to system notification settings.
- Request system permission after the user explicitly enables notifications.
- Confirm preferences against the Server before reporting them as saved.
- Buffer early delegate callbacks and notification taps until stores and
  authentication are ready.
- Reconcile authorization and registration on launch, foreground entry, and
  network recovery. Preserve registration across ordinary token refresh.
- Scope registration work to the captured deployment/account/session and
  reject stale results after logout or account changes.
- Receive one foreground system banner; do not add a second live-event
  banner for the same event.
- Reject navigation from an old deployment or account. Fall back to a safe
  authenticated list when the target no longer exists.

## Registration, encryption, and relay admission

- Create per-installation notification content keys and keep the device
  copy in shared Keychain storage available to a Notification Service
  Extension. Send the Server's copy only through authenticated registration.
- Use a versioned AES-256-GCM envelope and shared interoperability vectors
  across Rust and Swift. Bind deployment/account, event identity, category,
  and navigation target inside the encrypted content.
- Carry the actual APNs environment with registration. Development tokens
  and distribution tokens must use their corresponding Apple endpoints.
- Use App Attest with one-time challenges to authorize relay registration.
  Verify the Apple trust chain, app identity, environment, nonce, and key;
  verify subsequent assertions and reject replay.
- Issue narrowly scoped delivery grants for the registered device and
  environment. Refresh and revocation must not authorize another device.
- Keep relay state for challenge/replay/revocation and admission limits;
  do not store notification content keys or plaintext messages there.
- A relay failure or unsupported App Attest leaves push unavailable with
  visible status; it does not disable login or monitoring, and does not
  fall back to public production admission.
- Use an isolated ServerBee relay deployment with its own topic and
  configuration. Do not modify or deploy the existing Heeler relay.

The relay can observe device tokens, source IPs, timing, request size,
environment, authorization identifiers, and ciphertext. Explain this
before permission is requested and in the mobile documentation.

## Server lifecycle and delivery

- Persist subscriptions and registration ownership with a stable
  installation identity and a revision for token/key/grant changes.
- Rotate authentication credentials atomically while retaining the
  installation's valid push binding. Logout, device revocation, password
  changes, user deletion, and session expiry stop eligible delivery.
- Persist one pending delivery per logical event and eligible installation,
  with an expiry 30 minutes after the event. Store encrypted content for
  queued delivery; avoid persisting a second plaintext message history.
- Retry transient network, rate-limit, and provider failures with bounded
  timeouts and backoff. Discard expired deliveries after restart or outage.
- Revalidate subscription, role, session validity, and registration revision
  before a send. A stale response must not delete a replacement token.
- Classify APNs errors by reason. Configuration, payload, and environment
  errors are not blanket grounds for deleting device registrations.
- Separate accepted, retryable, permanent, and expired outcomes. APNs
  acceptance does not prove device presentation, and ambiguous network
  failures cannot promise exactly-once display.

## Implementation packages

1. Server registration/auth lifecycle, subscription APIs, migrations,
   encrypted envelope, outbox/worker, event wiring, and behavior tests.
2. iOS notification settings, App Attest registration, shared Keychain,
   notification extension, callback buffering, identity-aware routing,
   localization, and tests.
3. Isolated Relay admission, scoped grants, revocation, APNs transport,
   request limits, tests, and deployment documentation.
4. Integration verification, bilingual mobile/configuration documentation,
   API type generation where contracts change, and focused local commits.

Use established cryptography implementations rather than handwritten
cryptographic primitives or certificate validation. Proposed additional
libraries are Rust `aes-gcm` for the envelope and Relay `cbor-x` plus
`@peculiar/x509` for App Attest's CBOR and certificate boundary. Confirm
runtime/version support before installation. Their addition remains part
of final design confirmation.

## Verification and handoff

- Server integration tests: refresh preserves registration; logout and
  revocation stop delivery; role/category/expiry filtering; task retries
  produce only a final summary; security-rule suppression; queue expiry,
  restart, provider failure, and token replacement races.
- Relay tests: valid and invalid attestation/assertion fixtures, replay,
  grant scope/revocation, APNs environment and headers, size limits, error
  classification, and JWT caching. Mock only Apple/network boundaries.
- iOS tests: permission states, callback buffering, authenticated
  registration, failed preference saves, identity/routing, and encrypted
  envelope fixtures. Regenerate the Xcode project and run relevant builds
  and tests on an explicitly selected simulator.
- Relevant Rust formatting/Clippy, client types, docs contracts, and
  navigation checks follow repository entry points.
- Separately verify real-device App Attest and APNs delivery, foreground,
  background, terminated launch, notification decryption, refresh, logout,
  and taps when a configured test relay and signed device build exist.

This implementation request authorizes local code, tests, documentation,
and commits. Relay deployment, secret installation, Apple account changes,
pushes, PR creation, and production configuration changes require a
separate explicit request. The handoff must distinguish completed local
verification from any blocked live-device or deployed-service checks.

## References

- [ADR-0005](../adr/0005-mobile-notifications-via-push-relay.md)
- [APNs registration](https://developer.apple.com/documentation/usernotifications/registering-your-app-with-apns)
- [APNs signing keys](https://developer.apple.com/help/account/capabilities/communicate-with-apns-using-authentication-tokens/)
- [Notification Service Extension](https://developer.apple.com/documentation/usernotifications/modifying-content-in-newly-delivered-notifications)
- [App Attest](https://developer.apple.com/documentation/devicecheck/establishing-your-app-s-integrity)
- [App Attest server validation](https://developer.apple.com/documentation/devicecheck/validating-apps-that-connect-to-your-server)
