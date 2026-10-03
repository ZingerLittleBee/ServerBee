# Deliver mobile notifications through a developer-operated Push Relay

## Status

Accepted (2026-10-03). The approved requirements in #196 and implementation tickets #197–#205 authorize this decision. Local implementation and automated boundary evidence remain separate from pending genuine App Attest and APNs device acceptance.

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

The official relay verifies App Attest before issuing an installation-bound
delivery grant. ServerBee does not copy Heeler's public, unauthenticated
push admission. Authorization is scoped to the registered device and APNs
environment; neither device attestation nor a delivery grant replaces the
Server's user/session/role checks.

Official relay registration fails closed when App Attest is unsupported or
verification fails. The app exposes retryable push status while monitoring
and authentication remain available. Development testing uses an isolated
relay configuration rather than bypassing production admission.

Users explicitly enable categories in iOS Settings before the system
notification permission prompt. Foreground delivery uses the system banner
without a second WebSocket-driven banner. Taps open the appropriate alert,
security, or task-run detail only when deployment and user identity match
the current login. The first release retains the app's single-Server login
model.

Heeler provides a reference for the relay and iOS lifecycle, but its SSH-based
device registration and unauthenticated relay endpoint are not ServerBee's
user ownership or delivery authorization model.

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
