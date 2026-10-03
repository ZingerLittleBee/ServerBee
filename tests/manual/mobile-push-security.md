# Administrator security notification acceptance

Use an isolated configured ServerBee Relay and a correctly signed app. Do not
use Heeler's Relay, existing user Simulators or production credentials. Record
the Server/app/Relay revisions and APNs environment with each observation.

## Automated boundaries

- Server: `cargo test -p serverbee-server --test mobile_push_integration security_push`
  uses real HTTP mobile login, confirmed subscriptions, rule writes, migrated
  SQLite, a registered Agent WebSocket and security event evaluation. Only the
  external Relay boundary is substituted. A real SQLite trigger failure verifies
  admission rollback, unchanged cooldown/cache, no partial fan-out or browser
  publication, and successful retry after database reopen. Existing `mobile_push_integration`
  cases retain registration ownership, revocation and retry coverage.
- Native: run `EncryptedPushNavigationTests`, `SecurityNotificationDetailTests`
  and `NotificationServiceTests` in `ServerBeeTests`. These verify encryption,
  cold/warm callback routing (including already-presented notifications tapped
  after 30 minutes), expired extension presentation, current-key rejection of old
  account/deployment/installation/login envelopes and current-role/target validation, substituting
  authenticated HTTP/system boundaries. They do not establish real App Attest,
  APNs receipt, device presentation or observed navigation.
- Run the shared Rust/Swift security envelope vector and English/zh-Hans
  localization checks. Regenerate the ignored Xcode project with `xcodegen generate`.

## Real-device observations (pending)

1. Enable **Security rule matches** as an administrator. Confirm the saved
   preference, genuine App Attest grant and independent permission status.
   A member must have no security subscription control and Server saves must
   reject that category even if the cached role is stale.
2. With no notification group, admit SSH new-IP login, brute-force and port-scan
   events through enabled rules. Check thresholds, case-insensitive username
   exclusions, CIDR/bare-IP exclusions, Server coverage, maintenance and cooldown.
   Routine raw detections must stay silent. Multiple matching rules produce one
   logical security event per eligible installation, without an alert duplicate.
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
