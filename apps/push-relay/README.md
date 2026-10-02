# ServerBee Push Relay admission

This package implements verified device registration, renewal, grant inspection
and revocation for #198. APNs transport and category delivery are subsequent
implementation gates. There is no send endpoint in this package yet. Setup
confirmation must not be advertised as working notification delivery.

## Isolated configuration

Use Bun (the repository package manager) and OpenSSL 3 on the Relay host. Install
with `bun install --frozen-lockfile` from the repository root, then run
`bun --filter @serverbee/push-relay start`. Keep this instance, topic and database
separate from Heeler. Never point fixtures or development clients at a production
Relay. No deployment or credential installation is part of this ticket.

Required environment variables:

| Name | Meaning |
| --- | --- |
| `RELAY_DATABASE` | Persistent SQLite file, with a private directory and backups |
| `APP_ATTEST_ROOT_CA` | Path to Apple's App Attest root certificate PEM |
| `APP_ATTEST_ROOT_SHA256` | Audited colon-separated SHA256 certificate fingerprint |
| `APP_ATTEST_APP_ID` | App ID prefix plus `.` plus the official bundle identifier |
| `APP_ATTEST_BUNDLE_VERSIONS` | Comma-separated approved `CFBundleVersion` values for signed extension claims |
| `APNS_ENVIRONMENTS` | Allowed environments; `sandbox,production` admits both on one URL/database |
| `APNS_ENVIRONMENT` | Single-environment alternative when `APNS_ENVIRONMENTS` is absent |
| `RELAY_PORT` | Optional loopback listener port, default `8787` |

Obtain and verify the App Attest root from Apple's published trust material.
Pin the fingerprint through a separate trusted review, not from an incoming
request or an untrusted certificate chain. The configured root is the only
trusted CA; the operating system TLS trust store does not admit devices. A
changed pin must be an intentional operator action. Test fixtures supply an
isolated test CA directly to the handler constructor; the executable exposes no
skip-verification flag or alternate admission path.

The listener binds `127.0.0.1`. A TLS reverse proxy must enforce source-IP rate
limits, a 32 KiB body limit, connection/time limits and bounded concurrency. The
handler also caps streamed bodies, pending challenges and requests per connection
source. It deliberately ignores caller-supplied forwarding headers; configure
proxy limits using the proxy's actual trusted peer chain. Grant inspection and
proof responses have `Cache-Control: no-store`. Do not log bodies, bearer grants,
attestations or receipts. SQLite stores hashes of grant bearer tokens, not their
plaintext. Persist the database through restarts so counters cannot reset.

Configure the self-hosted Server with `SERVERBEE_PUSH_RELAY__URL` or
`[push_relay].url`. Use HTTPS with a trusted certificate. Only the Server's local
HTTP integration harness permits a loopback HTTP Relay. Apple signing credentials
must never be installed on a self-hosted Server.

APNs transport will consume publisher-only `APNS_TEAM_ID`, `APNS_KEY_ID`,
`APNS_PRIVATE_KEY` and `APNS_TOPIC`. They are credential names, not values; this
package does not consume these signing inputs or send APNs notifications yet.

## Signing and environment

The app requires APNs and App Attest capabilities in its provisioning profile.
The project pairs Debug `aps-environment=development` with App Attest
`development` and the API's `sandbox` value. Release uses `production` for both
entitlements and the API. Check the **signed artifact's** effective entitlements
before live validation; build settings alone do not establish valid Apple
provisioning. TestFlight/App Store builds use production attestation and APNs.
Use distinct isolated Relay instances/databases for development and production.

The official workflow keeps publisher credentials at the Relay. Official-app
users need neither a personal Apple developer account nor their own APNs key.
Source builds still require an appropriate signing identity; the repository does
not currently publish a downloadable official signed artifact.

## Protocol

All endpoints use POST. JSON responses here are direct objects, whereas the
Server API wraps responses in `data`.

1. `/v1/challenges` accepts `action` (`attest`, `renew`, `revoke`), `key_id`,
   `device_token` and `environment`. Revocation also requires `grant_id`.
   It returns `challenge_id` and base64 `client_data`. Hash the decoded bytes
   unchanged with SHA256 for App Attest. Those one-time bytes include the action,
   key, device token, environment, optional grant and random nonce. Challenges
   expire in five minutes and are consumed even by rejected verification attempts.
2. `/v1/attest` accepts `challenge_id` and base64 `proof`. The proof is Apple's
   attestation object. Validate CBOR, the certificate path and validity, the
   Apple nonce extension, app identity, environment, credential/public/COSE key
   binding and zero counter. A known key cannot be re-attested to reset its counter.
3. `/v1/renew` accepts an assertion proof over a fresh renewal challenge. Validate
   its signature, RP ID and strictly increasing persistent counter. Renewal can
   update the APNs token and rotates all earlier grants for that attested key.
4. Successful admission/renewal returns `grant_id`, `grant_token`, `key_id`,
   `device_token`, `environment` and Unix `expires_at`. Grants last 24 hours.
   The app passes the grant only to its captured authenticated Server context.
5. `/v1/grants/inspect` requires the bearer grant. It returns the exact registered
   scope and expiry, without the bearer token. The Server compares all scope
   fields before storing a registration, then revalidates session/revision under
   the database writer lock. No inspection accepts a revoked or expired grant.
6. `/v1/revoke` requires a signed assertion over a challenge containing that
   specific `grant_id`. It revokes only the matching key/device/environment/grant,
   so delayed cleanup cannot revoke a replacement grant. Server logout/session
   revocation independently removes its registration even if Relay cleanup fails.

Server subscription APIs:

- `GET /api/mobile/push/settings`: current mobile login's confirmed intent,
  revision, registration/expiry and Relay URL; no grant bearer or APNs token.
- `PUT /api/mobile/push/settings`: `expected_revision` and all category booleans
  inside `preferences`. Security requires an administrator. Disable clears the
  Server grant immediately. Re-enable requires fresh proof.
- `POST /api/mobile/push/verified-register`: `expected_revision`, `device_token`,
  `environment`, `key_id`, `grant_id`, `grant_token`. Requires explicit enabled
  intent and a valid grant inspected through the configured Relay.
- `POST /api/mobile/push/unregister`: cleanup remains scoped to user, installation
  and mobile session. Database foreign keys cascade logout/device/account removal.

A registration tied to another still-active login cannot be adopted just by
presenting the installation identifier, even for the same username. Revoke that
old paired session before re-enabling on a new login. Routine access-token refresh
retains the mobile session and registration.

## Verification

Run `bun --filter @serverbee/push-relay test` and `bun --filter
@serverbee/push-relay typecheck`. Relay tests use real handlers, migrated local
SQLite and certificate/assertion fixtures signed by an isolated test CA. OpenSSL
performs actual path and validity validation. Those generated Apple-format
fixtures are **not genuine Apple device proof**.

Server checks use `cargo test -p serverbee-server --test mobile_push_integration
--test router_mobile`. They exercise real HTTP/authentication and migrated SQLite;
only the external Relay response is replaced. iOS system-boundary tests replace
permission/token, App Attest and HTTP services, retaining the real manager and
authentication generation checks.

Live acceptance requires an isolated configured Relay and correctly provisioned
physical device. Record genuine App Attest admission, wrong-environment rejection,
renewal and revocation separately. When subsequent delivery packages are ready,
record APNs provider acceptance, observed foreground/background/terminated
presentation and authenticated tap navigation as three distinct layers. Missing
Apple credentials, signing, or a physical device cannot be compensated for by a
successful build, Simulator run or fixture test.

Protocol reference: [Apple App Attest server validation](https://developer.apple.com/documentation/devicecheck/validating-apps-that-connect-to-your-server).

### Renewal recovery and login isolation

A Relay renewal immediately invalidates that key's earlier grants. Server settings
inspect the current grant instead of inferring validity from its saved expiry. If
Relay inspection fails, setup is unconfirmed, while subscription intent remains
saved. The iOS client persists a newly obtained grant in Keychain until Server
confirmation; foreground, connectivity and restart recovery reuse that grant with
a fresh Server revision instead of rotating it again. No pending grant appears as
confirmed before the authenticated Server response succeeds.

App Attest key storage is scoped to deployment, installation and paired login,
including replacement logins for the same account. The login scope is a local
hash of the captured deletion proof and is never sent to Relay. Identity checks
also run after challenge/native-proof completion, before sending the mutation.
Native `invalidKey` errors discard that login's key and marker; transient network
or Apple `serverUnavailable` errors retain the key for retry.

Current assertions may include a signed CBOR extension dictionary following the
37-byte header. Validate the complete authenticator data signature, extension flag
and exact CBOR framing, distribution category and approved bundle version. Legacy
assertions without extensions remain supported. Apple's attestation validation
vector represents the category as a four-byte little-endian UInt32; assertion
extension names follow the current validation guide (`validationCategory` and
`bundleVersion`). Distribution is restricted to development for sandbox and
TestFlight/App Store for production. Keep the version allowlist current when
admitting a newly published build.

### Sandbox and production on one deployment

Set `APNS_ENVIRONMENTS=sandbox,production` to admit both kinds of installation on
one HTTPS Relay URL and one SQLite database. A single-environment deployment may
still use `APNS_ENVIRONMENT`; the plural setting takes precedence. Existing
Server and iOS API paths remain the same. The challenge endpoint validates the
requested environment against the configured set and persists it with the
challenge; proof validation uses that saved environment for AAGUID/distribution
checks. Assertions must use the previously attested key's environment. Grants
retain their device/environment scope; renewal or revocation of a sandbox key
cannot affect a production key. Grant inspection locates the opaque bearer in
that shared database and returns its verified environment for Server comparison.
This enables admission only, not APNs delivery.
