# ServerBee public push Relay (Cloudflare Workers)

The Relay has one public endpoint: `POST /v1/send`. It validates an opaque
Server→phone AES-256-GCM envelope, signs an APNs provider JWT with WebCrypto,
and forwards it using Workers `fetch`. No runtime packages, Bun runtime,
Node HTTP/2 transport, OpenSSL, containers, accounts, database, device grants,
App Attest, admission/version policy, queue or retry scheduler are required.
The Server owns authenticated registration, durable delivery and retries.
The iOS extension owns decryption. Provider acceptance does not prove presentation.

## Quick setup

Prerequisites: Node 24, the repository's Bun package-manager version, a Cloudflare
Workers account, an APNs `.p8` signing key and the correct app bundle ID/topic.
From the repository root:

```sh
bun install --frozen-lockfile
bun --filter @serverbee/push-relay typecheck
bun --filter @serverbee/push-relay test
bun --filter @serverbee/push-relay build   # dry-run only; no deployment
```

Edit `apps/push-relay/wrangler.jsonc` with your Worker name. The official app
Bundle ID and `APNS_TOPIC` are `app.serverbee`; the embedded extension is
`app.serverbee.notifications` and must never be used as the APNs topic.
Keep `enable_request_signal` in `compatibility_flags`: it wires incoming client
cancellation into the Worker’s upload/APNs deadlines and cleanup. A compatibility
date alone does not enable this opt-in flag.
`APNS_ENVIRONMENTS` is an optional comma-separated allowlist: `sandbox`,
`production`, or `sandbox,production` (the default). Empty, duplicate or unknown
values fail closed. It does not infer app authenticity or distribution channel.

Set these Worker secrets through your own authorized Cloudflare account:

```sh
cd apps/push-relay
bunx wrangler secret put APNS_TEAM_ID
bunx wrangler secret put APNS_KEY_ID
bunx wrangler secret put APNS_PRIVATE_KEY   # paste the complete PKCS#8 .p8 PEM contents
bun run deploy
```

Team and key IDs must each be 10 uppercase letters/digits. They may also be
ordinary Wrangler variables; the private key must remain a secret. The topic comes only
from deployment configuration, never a request. Keep the `.p8` outside the repo;
do not paste it into logs or tracked files. `APNS_PRIVATE_KEY` is PEM contents,
not a filesystem path. For `bun run dev`, put these same values in an ignored
`.dev.vars` file. Do not expose the local development server publicly: only
Cloudflare ingress makes `CF-Connecting-IP` trustworthy. Set the resulting HTTPS
Worker origin as the Server's Relay URL. No SQLite volume or migration is needed.
This repository does not deploy or configure an account automatically.

## Wire contract

Use `Content-Type: application/json`. The exact request fields are:

```json
{
  "device_token": "<2-1024 lowercase hexadecimal characters, even length>",
  "environment": "sandbox",
  "event_id": "11111111-1111-4111-8111-111111111111",
  "expires_at": 2000001800,
  "envelope": {
    "version": 1,
    "key_id": "22222222-2222-4222-8222-222222222222",
    "identity": "<64 lowercase hexadecimal characters>",
    "nonce": "<canonical standard base64 encoding of 12 bytes>",
    "ciphertext": "<canonical standard base64 ciphertext plus GCM tag>"
  }
}
```

There is no Authorization header or issuance/inspection/renewal/revocation API.
Unknown top-level and envelope fields are rejected. UUIDs are lowercase;
`expires_at` is a positive integer Unix timestamp at most 30 minutes ahead.
Already-expired valid requests return `expired` without contacting APNs.

APNs device tokens have variable length, including longer Simulator tokens.
Server and Relay accept complete lowercase hex byte encodings up to 1024
characters as a resource limit and forward them unchanged. The envelope identity
remains exactly 64 characters.
Ciphertext must decode to 16–2070 bytes (maximum 2760 base64 characters).
The unchanged envelope is placed in `serverbee_envelope`; the only plaintext
alert is `ServerBee` / `Open ServerBee to view this notification.`, with
`mutable-content: 1` and `sound: default`. A caller cannot set alert text,
APNs hosts, topic, push type, priority, collapse ID or arbitrary headers.

APNs destinations are fixed to `api.sandbox.push.apple.com` and
`api.push.apple.com`. Redirects are not followed. The validated event UUID is
`apns-id`, and expiry is `apns-expiration`. The JWT uses ES256/P-256 and is cached
for 50 minutes with single-flight signing and clock-rollback refresh.

All responses carry `Cache-Control: no-store` and this shape:

```json
{"outcome":"accepted","reason":"Accepted","device_invalid":false}
```

- `200 accepted`: APNs returned 200 with an empty body
- `200 permanent` with `Unregistered` and `device_invalid: true`: only the exact
  APNs `410` + `Unregistered` pair; the Server must still fence by registration
  revision before invalidating a device
- Other APNs 4xx/redirect/configuration/payload failures: `permanent` and false;
  `BadDeviceToken` becomes `DeviceOrEnvironmentMismatch`
- APNs 429/5xx: `retryable` / `ProviderUnavailable`; network, timeout, oversized,
  malformed or contradictory provider responses: `retryable` / `NetworkUnavailable`
- `200 expired`: the event expired before forwarding
- Validation errors: 400/413/415 `permanent`; upload timeout: 408 `retryable`;
  throttling: 429 `retryable`; concurrency saturation: 503 `retryable`
- Missing or invalid deployment configuration: 503 `permanent` /
  `RelayNotConfigured`; an invalid signing key returns the same verdict with 200
- Removed routes: 404; wrong method on `/v1/send`: 405

Treat a `retryable` response as backoff advice subject to the original event
expiry. Retrying uses the same event ID; the Relay does not queue or deduplicate.
A notification accepted by APNs may never appear on the device.

## Public-endpoint risk and bounded resource use

This is intentionally a public, unauthenticated forwarder. A valid device token
is not proof of ownership or consent. Someone who obtains one can send encrypted
garbage, repeat events or trigger the generic fallback alert and consume provider
quota. Arbitrary invalid requests, or sends to an attacker’s own valid token,
can consume Worker/request quota without knowing any victim’s token. The Relay cannot prove the origin, distinguish legitimate Server instances,
revoke callers, prevent distributed abuse, or promise a global rate limit.
Encryption hides content but does not authorize delivery. Accept those risks
before publishing the Worker. Cloudflare zone/WAF controls and account spending
alerts may help operations, but are not replaced by this code.

Built-in guardrails, held only in each isolate's volatile memory:

- 8 KiB streamed request cap checked before full read/JSON parsing, including
  chunked uploads and dishonest Content-Length; a 5-second whole-upload deadline
- Strict schema, canonical base64 and decoded-size checks; 4096-byte APNs payload
- 10-second APNs deadline spanning signing, headers and body; provider body cap
  4096 bytes (zero on success); abort/cancel on timeout, client abort or overflow
- At most 32 in-flight uploads/deliveries and no unbounded waiting queue
- Fixed-minute limits: 120 requests/IP, 60 sends/environment+device token,
  600 requests/isolate; Retry-After on throttling/saturation
- Each IP/target map has a hard 4096-entry cap. When all slots are live, new keys
  are rejected rather than evicting active limits or growing the map
- Only Cloudflare's `CF-Connecting-IP` is considered; absent/invalid values share
  the `unknown` bucket. `X-Forwarded-For` and `X-Real-IP` are ignored

Multiple isolates/regions, isolate restarts and fixed-window boundaries can bypass
or multiply these best-effort limits; callers can also exhaust slots and deny
service to legitimate users. Counters and JWT cache are ephemeral, not durable
state. No tokens, payloads, keys or raw upstream errors are logged by the Worker;
observability is disabled by default. Avoid adding sensitive request logging.

## Verification and the Server's encrypted test notification

`bun --filter @serverbee/push-relay test` runs Vitest inside actual workerd using
Cloudflare's official Workers Vitest pool, including real WebCrypto, stream cancellation,
upload/provider deadlines, map hard caps, schema validation, per-IP/target/global
limits, concurrency and mocked APNs fetch. A second Node-driven test sends real
HTTP requests to the bundled Worker’s native workerd ingress, saturates the
32-request limit, resets one TCP connection during a stalled APNs request, and
verifies a new request can proceed before the 10-second provider deadline. It
reads the actual Wrangler compatibility flags and always disposes the runtime. `build` uses Wrangler's production
bundle dry-run without `nodejs_compat`. CI runs both as well as type checking. The pool, Wrangler and Miniflare versions
are pinned to the last stable Miniflare 4-compatible release set, avoiding the
Miniflare 5 alpha currently used by newer tooling.

`tests/serve-delivery-fixture.ts` starts this production Worker bundle in
Miniflare/workerd for the Rust stitched tests, replacing only the APNs network.
Its `ready.json` contains `{url, device_token, environment}`. Test provider controls
stay in `provider.json`, and captured `{token, environment, headers, payload}` stay
in `provider-request.json` and `provider-requests.jsonl`. These contain only
isolated test data and are never production logging. The shared AES fixture is
`tests/fixtures/push-envelope-v1.json` at the repository root.

After the app registers through the authenticated Server endpoint
`POST /api/mobile/push/encrypted-register`, a test send targets that installation:

```sh
curl -X POST "$SERVER/api/mobile/push/test" \
  -H "Authorization: Bearer $MOBILE_ACCESS_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"expected_revision":2,"event_id":"11111111-1111-4111-8111-111111111111"}'
```

`expected_revision` guards against stale setup/token/key changes and `event_id`
identifies the exact test event. Genuine APNs acceptance, iOS extension decryption,
notification presentation and tap navigation still require separate real-device
checks; passing local mocks or Simulator tests cannot establish those outcomes.
