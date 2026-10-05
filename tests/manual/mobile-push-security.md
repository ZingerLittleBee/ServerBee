# Administrator security notification acceptance

Use an isolated configured ServerBee Relay and a correctly signed app. Do not
use Heeler's Relay, existing user Simulators or production credentials. Record
the Server/app/Relay revisions and APNs environment with each observation.

## Automated boundaries

- Server: `cargo test -p serverbee-server --test mobile_push_integration security_push`
  uses real HTTP mobile login, confirmed subscriptions, rule writes, migrated
  SQLite, a registered Agent WebSocket and security event evaluation. Only the
  external Relay/Apple and configured webhook boundaries are fixtures. Real
  Production WAL/NORMAL/foreign-key/5-second busy settings apply to every pooled
  connection and database reopen. SQLite faults cover raw writes, rule/intent commit and a second recipient's
  outbox INSERT. Once-only WS cases verify raw/browser/firewall/external
  preservation, suppression rollback before durable intent, atomic fan-out,
  original UUID/facts/deadline, current recipient checks, responsive reports/Pong/connection replacement/closure/revocation while raw storage fails, cancellation of unauthorized recovery, maintenance decisions across window starts/ends and lookup failures, and automatic recovery
  after faults and database reopen, without another detection or service call.
  Competing writers must preserve durable revocation and old-token rejection;
  failed history commits retain the valid token, history and live connection.
  Multi-event cases retain original timestamps across memory/snapshot/admission
  faults, let unrelated keys and both installations progress, and verify A/B
  delivery with C suppression plus monotonic SQLite cooldown after reopen. Before the first successful raw write, retention is service-owned memory; process-death recovery is not guaranteed at that boundary. Existing `mobile_push_integration`
  cases retain registration ownership, revocation and retry coverage.
- Native: run `EncryptedPushNavigationTests`, `SecurityNotificationDetailTests`
  and `NotificationServiceTests` in `ServerBeeTests`. These verify encryption,
  cold/warm callback routing (including already-presented notifications tapped
  after 30 minutes), expired extension presentation, current-key rejection of old
  account/deployment/installation/login envelopes and current-role/target validation, substituting
  authenticated HTTP/system boundaries. They do not establish live APNs provider acceptance,
  APNs receipt, device presentation or observed navigation.
- Run the shared Rust/Swift security envelope vector and English/zh-Hans
  localization checks. Regenerate the ignored Xcode project with `xcodegen generate`.

## Real-device observations (pending)

1. Enable **Security rule matches** as an administrator. Confirm the saved
   preference, authenticated Server registration and independent permission status.
   A member must have no security subscription control and Server saves must
   reject that category even if the cached role is stale.
2. With no notification group, admit SSH new-IP login, brute-force and port-scan
   events through enabled rules. Check thresholds, case-insensitive username
   exclusions, CIDR/bare-IP exclusions, Server coverage, maintenance and cooldown.
   Routine raw detections must stay silent. Multiple matching rules produce one
   logical security event per eligible installation, without an alert duplicate.
   Inject a storage fault before push queue insertion, then recover or restart
   without another detection. The original history, browser updates and existing
   external/firewall effects must remain, and recovery must retain the original
   event UUID and deadline without repeating suppression or external effects.
3. Observe foreground, background and terminated-app delivery on development
   and distribution devices. Record provider acceptance separately from banner
   presentation. Foreground WebSocket updates must not add a second banner.
4. Tap after warm and cold launch. Confirm the corresponding Server security
   feed and exact event sheet, including a notification presented more than
   30 minutes before the tap. Delivery expiry must not expire an authenticated
   security target; the app still checks current login, role and resources.
   Delete the event/Server or downgrade the user;
   confirm a dismissible unavailable screen. Switch deployment/account or
   replace the login; an older envelope must not navigate in the new context.
5. Repeat with two installations. Queue during a Relay outage, then unsubscribe,
   logout, revoke or downgrade before recovery. No later eligible send may cross
   the Relay boundary. Restart the Server and recover within the original
   30-minute deadline; expired events must not arrive. A request already in
   flight or accepted by APNs cannot be retracted, and ambiguous retries can
   duplicate presentation.
6. Repeat settings, notification rendering and fallback in English and zh-Hans.
   Tampering, wrong keys and unavailable extension Keychain access must show
   generic text without an untrusted plaintext target.
