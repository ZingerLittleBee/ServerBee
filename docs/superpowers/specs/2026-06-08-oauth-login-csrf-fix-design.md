# OAuth Login CSRF / Session Fixation Fix — Design

Date: 2026-06-08
Status: Approved
Area: `crates/server` — OAuth login flow

## Problem

The OAuth login flow does not bind its CSRF `state` to the initiating browser
and uses no PKCE. `oauth_authorize` stores `state -> (provider, created_at)` in a
server-global `DashMap` keyed only by the CSRF token value; `oauth_callback`
validates only that the state exists, the provider matches, and the 10-minute TTL
has not elapsed, then unconditionally issues a `session_token` cookie.

Because the state is never tied to the browser that started the flow, an attacker
can pre-initiate a flow with their own provider account, capture an unspent
`code` + `state`, and lure a victim to the callback URL within the TTL. The
victim's browser is then silently logged into the **attacker's** account
(login CSRF / session fixation). Any data, credentials, or 2FA the victim
subsequently configures lands in the attacker's account. This is the only
audit finding reachable with no credentials (only when an OAuth provider is
configured; providers are disabled by default).

This deviates from the OAuth 2.0 Security BCP (RFC 9700), which requires either
PKCE or a state value bound to the user agent.

## Goal

Make the OAuth login flow resistant to login CSRF / session fixation and align
it with the OAuth 2.0 Security BCP, without changing the frontend, adding
migrations, or introducing new configuration.

## Approach

Add browser binding via a short-lived pre-auth cookie nonce, and adopt PKCE
(S256):

1. On authorize, generate a random `nonce` (CSPRNG) and a PKCE
   challenge/verifier. Send the challenge to the provider, store the nonce and
   verifier alongside the state, and set the nonce in a short-lived HttpOnly
   pre-auth cookie.
2. On callback, require the pre-auth cookie nonce to match the stored nonce for
   the presented state before exchanging the code (with the PKCE verifier).

An attacker cannot set the victim's pre-auth cookie, so a forged callback fails
the nonce check. PKCE additionally defeats authorization-code interception /
injection.

The frontend already initiates the flow via a top-level anchor navigation
(`<a href="/api/auth/oauth/{provider}">`), so the `SameSite=Lax` pre-auth cookie
is sent both on the authorize redirect and on the provider's callback redirect.
No frontend change is required.

## Design

### Flow-state structure (`crates/server/src/state.rs`)

Replace the tuple value with a named struct:

```rust
pub struct OAuthFlowState {
    pub provider: String,
    pub created_at: DateTime<Utc>,
    pub nonce: String,         // must match the pre-auth cookie on callback
    pub pkce_verifier: String, // PKCE code_verifier secret
}
// oauth_states: DashMap<String, OAuthFlowState>
```

### `oauth_authorize` (`router/api/oauth.rs`)

- Generate PKCE: `let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();`
  and `auth_request.set_pkce_challenge(challenge)`.
- Generate a CSPRNG nonce (reuse `AuthService::generate_session_token()`).
- Insert `OAuthFlowState { provider, created_at: now, nonce, pkce_verifier:
  verifier.secret().clone() }` keyed by the CSRF state secret.
- Keep the existing expired-state eviction.
- Change the return type to `(HeaderMap, Redirect)` and set the pre-auth cookie:
  `oauth_nonce=<nonce>; HttpOnly; SameSite=Lax; Path=/api/auth/oauth; Max-Age=600`
  (append `; Secure` when `auth.secure_cookie`).

### Callback validation — extracted pure function

```rust
fn validate_and_consume_state(
    states: &DashMap<String, OAuthFlowState>,
    state_param: &str,
    provider: &str,
    nonce_cookie: Option<&str>,
) -> Result<OAuthFlowState, AppError>
```

Order (note: `remove` consumes the state, making it single-use / replay-safe):

1. `remove(state_param)` returns `None` -> `BadRequest("Invalid or expired OAuth state")`.
2. provider mismatch -> `BadRequest("OAuth state mismatch")`.
3. `now - created_at > 10min` -> `BadRequest("OAuth state expired")`.
4. `nonce_cookie` missing or not equal to stored nonce (constant-time compare)
   -> `BadRequest("OAuth session mismatch")`.
5. Return the `OAuthFlowState` (carrying `pkce_verifier`).

### `oauth_callback` (`router/api/oauth.rs`)

- Parse `oauth_nonce` from the `Cookie` header via a small local helper that
  mirrors `extract_session_cookie`.
- Call `validate_and_consume_state(...)` to obtain the `pkce_verifier`.
- Exchange the code with `.set_pkce_verifier(PkceCodeVerifier::new(flow.pkce_verifier))`.
- On success, set the `session_token` cookie **and** clear the pre-auth cookie
  (`oauth_nonce=; Max-Age=0; Path=/api/auth/oauth`).

## Testing

Unit tests on `validate_and_consume_state` (no network required):

- rejects an unknown state
- rejects a provider mismatch
- rejects an expired state
- rejects a missing nonce cookie
- rejects a mismatched nonce cookie
- accepts a valid state + matching nonce, returns the verifier, and a second
  call fails (replay protection)

Full network flow (token exchange, userinfo) stays manually verified, matching
the existing code, which has no OAuth integration tests.

## Decisions / scope

- Callback errors keep returning HTTP 400, consistent with the existing
  state-error behavior. A legitimate user whose pre-auth cookie expired
  (>10 min between click and callback) sees 400 — same effective window as the
  existing state TTL.
- The pre-auth cookie is scoped to `Path=/api/auth/oauth` to limit its surface.
- The OIDC dead-code path (`userinfo` unimplemented, always 500s) is left
  untouched — out of scope.
- No migration, no config change, no frontend change.
- Files touched: `crates/server/src/state.rs`,
  `crates/server/src/router/api/oauth.rs`.

## Out of scope (tracked separately)

Other audit findings (server-side SSRF in service-monitor checkers, missing
audit logging on backup/restore and user management, agent token in WS query
string, default `trusted_proxies` breadth) are not addressed here.
