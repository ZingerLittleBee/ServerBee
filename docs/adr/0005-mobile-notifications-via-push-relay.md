# Deliver mobile notifications through a developer-operated Push Relay

## Status

Accepted and revised (2026-10-03). The initial unreleased design used App Attest and stateful delivery grants. The approved simplification replaces that Relay admission layer with a public, stateless Cloudflare Worker. The approved requirements in #196 and implementation tickets #197–#205 retain Server account ownership, encryption, delivery reliability and native behavior. Live APNs and signed-device acceptance remain separate pending evidence.

## Decision

ServerBee Server originates mobile notifications and sends them through a
developer-operated Push Relay to APNs. The relay holds the APNs signing
credentials for the official iOS app; self-hosted Servers never receive those
credentials. This supports official app installations without requiring every
Server operator to sign a separate iOS app or obtain credentials for the
official app's Apple Developer team.

The initial event scope includes alert transitions, security events admitted
by existing security alert rules, and final outcomes of scheduled or manually
started scheduled tasks. Security rule filters, maintenance suppression, and
deduplication continue to apply. Intermediate task retry failures do not
notify; final failures notify by default, with successful outcomes available
as an opt-in.

Subscriptions are managed by category in the iOS app and confirmed by the
Server. Mobile delivery does not require an APNs channel or notification
group; existing external notification channels retain their configured
groups. Alert transitions notify subscribed users, security notifications
require an administrator, and task outcomes notify the manual initiator or
the automatic task's creator. Every send revalidates the user's role and the
mobile session's validity.

Each scheduled task run produces at most one summary notification after all
target Servers reach their final outcome. Intermediate retries do not
notify. The summary reports failures and links to the run's results.

The Server encrypts notification content for the installation, and an iOS
Notification Service Extension decrypts it before presentation. The relay
receives encrypted content and delivery metadata, never the content key.
Task summaries exclude commands and command output; authenticated task
details remain the place to inspect those.

Pending deliveries are persisted on the Server and retried within a
30-minute expiry window. Expired deliveries are discarded rather than
flooding the device after an outage. APNs acceptance is a provider receipt,
not proof that a device displayed a notification.

The Relay is one public `POST /v1/send` Cloudflare Workers endpoint. It accepts
an APNs token/environment, event identity, expiry and the existing AES-256-GCM
envelope. WebCrypto signs APNs JWTs and platform fetch manages connections.
Fixed APNs hosts, operator-owned topic/environment allowlist, generic fallback
text, bounded streaming reads, parsing, timeouts and concurrency limit exposure.
Source/target rate limits have hard bounded maps and an isolate-wide cap. They
are best effort, reset with isolates, and do not promise global quota enforcement.

There is no App Attest, signed-build/distribution policy, challenge, delivery
grant issuance/inspection/renewal/revocation, Relay database or replacement
authorization service. This deliberate tradeoff accepts resource/cost abuse and
generic-notification harassment: an attacker knowing a valid device token can
submit junk or replay ciphertext; consuming Relay quota does not require knowing
someone else's token. The NSE may leave the fixed generic alert visible if
content is invalid or processing expires. AEAD prevents decrypting or forging
business content without the per-installation key, not public endpoint abuse.

Content keys are registered only through authenticated HTTPS Server requests.
The Server keeps user/installation/mobile-session ownership, role and revision
checks, durable ciphertext/retry and original expiry. The app keeps Keychain
isolation, logout/account-switch key cleanup and authenticated tap navigation.
Exact-target mobile-session revocation and offline logout cleanup remain because
they protect account access independently of the removed Relay grant registry.

Users explicitly enable categories in iOS Settings before the system
notification permission prompt. Foreground delivery uses the system banner
without a second WebSocket-driven banner. Taps open the appropriate alert,
security, or task-run detail only when deployment and user identity match
the current login. The first release retains the app's single-Server login
model.

Heeler provides the public stateless transport reference. ServerBee retains its
own authenticated HTTPS mobile-registration and account-ownership model; no SSH
registration or new public credential issuer is introduced.

## Considered Options

- **Every Server connects directly to APNs:** suitable for separately signed
  apps, but operators cannot provision signing credentials for the official
  app. Distributing the official app's private key with a public Server is
  unacceptable.
- **Server-originated delivery through a shared relay:** keeps Apple
  credential custody with the app publisher while retaining event selection
  and user ownership in each Server. Notifications depend on relay
  availability, and the relay's metadata and content visibility must be
  explicit.

## Operational verification

[English](../../apps/docs/content/docs/en/push-relay.mdx) and [Chinese](../../apps/docs/content/docs/zh/push-relay.mdx) runbooks cover isolated Relay configuration, effective signing entitlements, metadata visibility, troubleshooting and genuine-device records. [Combined integration checks](../../tests/manual/mobile-push-integration.md) retain real HTTP/session/subscription and migrated SQLite seams. Delivery and new extension rendering enforce the original 30-minute deadline; an already-presented authentic notification remains tappable afterward, with current Server authorization and all cryptographic, identity, size and target checks. No deployment, secret installation or Apple-account mutation follows from this decision.
