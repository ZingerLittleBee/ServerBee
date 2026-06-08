# OAuth Login CSRF / Session Fixation Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bind the OAuth login flow to the initiating browser (pre-auth cookie nonce) and adopt PKCE (S256) so a victim cannot be silently logged into an attacker's account.

**Architecture:** `oauth_authorize` mints a CSPRNG nonce + PKCE challenge, stores them with the CSRF state in the in-memory `oauth_states` map, and mirrors the nonce into a short-lived HttpOnly pre-auth cookie. `oauth_callback` consumes the state atomically and rejects the request unless the request's `oauth_nonce` cookie matches the stored nonce, then exchanges the code with the PKCE verifier.

**Tech Stack:** Rust, Axum 0.8, `oauth2` v4.4.2, `dashmap`, sea-orm.

Spec: `docs/superpowers/specs/2026-06-08-oauth-login-csrf-fix-design.md`

---

## File Structure

- `crates/server/src/state.rs` — add `OAuthFlowState` struct; change `oauth_states` field type.
- `crates/server/src/router/api/oauth.rs` — pure validation helper + cookie helpers + handler wiring + unit tests.

No migration, no config change, no frontend change. The two endpoints stay HTTP 302 redirects, so the generated OpenAPI / `api-types.ts` is unaffected.

---

## Task 1: Browser-bound OAuth state via pre-auth cookie nonce

This task fully closes the login-CSRF / session-fixation hole. PKCE is added in Task 2.

**Files:**
- Modify: `crates/server/src/state.rs` (struct near `PendingTotp` ~line 24; field ~line 56)
- Modify: `crates/server/src/router/api/oauth.rs`
- Test: `crates/server/src/router/api/oauth.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Add `OAuthFlowState` struct and migrate the field in `state.rs`**

Add the struct just after the `PendingTotp` struct definition:

```rust
/// In-flight OAuth login flow state, keyed by the CSRF `state` token.
///
/// `nonce` is mirrored into a short-lived HttpOnly pre-auth cookie set on the
/// authorize redirect and re-checked on the callback, binding the flow to the
/// browser that initiated it (defends against login CSRF / session fixation).
pub struct OAuthFlowState {
    pub provider: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub nonce: String,
}
```

Change the field declaration (was `DashMap<String, (String, chrono::DateTime<chrono::Utc>)>`):

```rust
    /// In-flight OAuth login flows, keyed by the CSRF `state` token.
    pub oauth_states: DashMap<String, OAuthFlowState>,
```

The initializer `oauth_states: DashMap::new(),` stays as-is.

- [ ] **Step 2: Write the failing unit tests in `oauth.rs`**

Append this module at the end of `crates/server/src/router/api/oauth.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::OAuthFlowState;
    use dashmap::DashMap;

    fn make_states(
        state: &str,
        provider: &str,
        nonce: &str,
        age_min: i64,
    ) -> DashMap<String, OAuthFlowState> {
        let states = DashMap::new();
        states.insert(
            state.to_string(),
            OAuthFlowState {
                provider: provider.to_string(),
                created_at: Utc::now() - chrono::Duration::minutes(age_min),
                nonce: nonce.to_string(),
            },
        );
        states
    }

    #[test]
    fn rejects_unknown_state() {
        let states: DashMap<String, OAuthFlowState> = DashMap::new();
        let err =
            validate_and_consume_state(&states, "missing", "github", Some("n")).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn rejects_provider_mismatch() {
        let states = make_states("s1", "github", "nonce1", 0);
        let err =
            validate_and_consume_state(&states, "s1", "google", Some("nonce1")).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn rejects_expired_state() {
        let states = make_states("s1", "github", "nonce1", 11);
        let err =
            validate_and_consume_state(&states, "s1", "github", Some("nonce1")).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn rejects_missing_nonce_cookie() {
        let states = make_states("s1", "github", "nonce1", 0);
        let err = validate_and_consume_state(&states, "s1", "github", None).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn rejects_mismatched_nonce_cookie() {
        let states = make_states("s1", "github", "nonce1", 0);
        let err =
            validate_and_consume_state(&states, "s1", "github", Some("wrong")).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn accepts_valid_state_and_is_single_use() {
        let states = make_states("s1", "github", "nonce1", 0);
        let flow =
            validate_and_consume_state(&states, "s1", "github", Some("nonce1")).unwrap();
        assert_eq!(flow.provider, "github");
        // second use must fail: state was consumed (replay protection)
        let err =
            validate_and_consume_state(&states, "s1", "github", Some("nonce1")).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }
}
```

- [ ] **Step 3: Run the tests to confirm they fail (RED)**

Run: `cargo test -p serverbee-server --lib router::api::oauth`
Expected: compile error — `validate_and_consume_state` not found (and possibly `OAuthFlowState` import). This is the RED state.

- [ ] **Step 4: Add imports, cookie name, helpers, and the validator to `oauth.rs`**

Add to the top-of-file imports (alongside the existing `use` lines):

```rust
use crate::state::OAuthFlowState;
use dashmap::DashMap;
```

Add a module-level constant (near the top, after the imports):

```rust
/// Name of the short-lived HttpOnly pre-auth cookie that binds an OAuth login
/// flow to the browser that started it.
const OAUTH_NONCE_COOKIE: &str = "oauth_nonce";
```

Add these three functions (place them above `oauth_authorize`):

```rust
/// Extract the `oauth_nonce` pre-auth cookie value from the request headers.
fn extract_oauth_nonce(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|c| {
            let c = c.trim();
            c.strip_prefix("oauth_nonce=").map(|v| v.to_string())
        })
}

/// Constant-time byte comparison (no early return) for the nonce check.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Validate and atomically consume the stored OAuth flow state.
///
/// `remove` makes the state single-use (replay-safe). Returns the flow state on
/// success, or a `BadRequest` for the first failed check.
fn validate_and_consume_state(
    states: &DashMap<String, OAuthFlowState>,
    state_param: &str,
    provider: &str,
    nonce_cookie: Option<&str>,
) -> Result<OAuthFlowState, AppError> {
    let (_, flow) = states
        .remove(state_param)
        .ok_or_else(|| AppError::BadRequest("Invalid or expired OAuth state".to_string()))?;

    if flow.provider != provider {
        return Err(AppError::BadRequest("OAuth state mismatch".to_string()));
    }
    if Utc::now() - flow.created_at > chrono::Duration::minutes(10) {
        return Err(AppError::BadRequest("OAuth state expired".to_string()));
    }
    match nonce_cookie {
        Some(cookie) if ct_eq(cookie.as_bytes(), flow.nonce.as_bytes()) => {}
        _ => return Err(AppError::BadRequest("OAuth session mismatch".to_string())),
    }
    Ok(flow)
}
```

- [ ] **Step 5: Rewrite `oauth_authorize` to mint the nonce, store the struct, and set the pre-auth cookie**

Replace the whole `oauth_authorize` function body (keep the `#[utoipa::path(...)]` attribute above it; change only the return type and body):

```rust
pub async fn oauth_authorize(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Result<(HeaderMap, Redirect), AppError> {
    if !OAuthService::is_configured(&provider, &state.config.oauth) {
        return Err(AppError::BadRequest(format!(
            "OAuth provider '{provider}' is not configured"
        )));
    }

    let client = OAuthService::build_client(&provider, &state.config.oauth)?;

    let mut auth_request = client.authorize_url(CsrfToken::new_random);

    // Add scopes based on provider
    let scopes = match provider.as_str() {
        "github" => vec!["read:user", "user:email"],
        "google" => vec!["openid", "email", "profile"],
        _ => vec![],
    };
    for scope in scopes {
        auth_request = auth_request.add_scope(Scope::new(scope.to_string()));
    }

    let (auth_url, csrf_token) = auth_request.url();

    // Browser-binding nonce: mirrored into a short-lived pre-auth cookie and
    // re-checked on callback to defend against login CSRF / session fixation.
    let nonce = AuthService::generate_session_token();

    state.oauth_states.insert(
        csrf_token.secret().clone(),
        OAuthFlowState {
            provider,
            created_at: Utc::now(),
            nonce: nonce.clone(),
        },
    );

    // Evict expired states (older than 10 minutes) to prevent memory leak
    let cutoff = Utc::now() - chrono::Duration::minutes(10);
    state.oauth_states.retain(|_, flow| flow.created_at > cutoff);

    let secure_flag = if state.config.auth.secure_cookie {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "{OAUTH_NONCE_COOKIE}={nonce}; HttpOnly; SameSite=Lax; Path=/api/auth/oauth; Max-Age=600{secure_flag}"
    );
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        SET_COOKIE,
        cookie
            .parse()
            .map_err(|_| AppError::Internal("Failed to set cookie".to_string()))?,
    );

    Ok((response_headers, Redirect::temporary(auth_url.as_str())))
}
```

- [ ] **Step 6: Rewrite the state-validation block in `oauth_callback` and clear the cookie on success**

In `oauth_callback`, replace the existing CSRF-validation block (the `let stored = state.oauth_states.remove(&query.state); match stored { ... }`) with:

```rust
    // Validate CSRF state + browser-binding nonce, atomically consuming the state.
    let nonce_cookie = extract_oauth_nonce(&headers);
    validate_and_consume_state(
        &state.oauth_states,
        &query.state,
        &provider,
        nonce_cookie.as_deref(),
    )?;
```

Then, at the end of `oauth_callback`, replace the cookie-setting block so it also clears the pre-auth cookie. Replace:

```rust
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        SET_COOKIE,
        cookie
            .parse()
            .map_err(|_| AppError::Internal("Failed to set cookie".to_string()))?,
    );

    Ok((response_headers, Redirect::temporary("/")))
```

with:

```rust
    // Clear the pre-auth nonce cookie now that the flow is complete.
    let clear_cookie = format!(
        "{OAUTH_NONCE_COOKIE}=; HttpOnly; SameSite=Lax; Path=/api/auth/oauth; Max-Age=0{secure_flag}"
    );

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        SET_COOKIE,
        cookie
            .parse()
            .map_err(|_| AppError::Internal("Failed to set cookie".to_string()))?,
    );
    response_headers.append(
        SET_COOKIE,
        clear_cookie
            .parse()
            .map_err(|_| AppError::Internal("Failed to set cookie".to_string()))?,
    );

    Ok((response_headers, Redirect::temporary("/")))
```

(`secure_flag` is already in scope in `oauth_callback` from the existing session-cookie code.)

- [ ] **Step 7: Run the tests to confirm they pass (GREEN)**

Run: `cargo test -p serverbee-server --lib router::api::oauth`
Expected: 6 tests pass.

- [ ] **Step 8: Build and lint**

Run: `cargo clippy -p serverbee-server -- -D warnings`
Expected: no warnings, no errors.

- [ ] **Step 9: Commit**

```bash
git add crates/server/src/state.rs crates/server/src/router/api/oauth.rs
git commit -m "fix(server): bind OAuth login state to initiating browser via pre-auth cookie"
```

---

## Task 2: Add PKCE (S256) to the OAuth login flow

Defense-in-depth against authorization-code interception/injection. The PKCE
verifier rides in the same `OAuthFlowState` and is bound to the same single-use
state, so no extra validation branch is needed.

**Files:**
- Modify: `crates/server/src/state.rs` (`OAuthFlowState`)
- Modify: `crates/server/src/router/api/oauth.rs`
- Test: `crates/server/src/router/api/oauth.rs` (update `make_states`)

- [ ] **Step 1: Add the `pkce_verifier` field to `OAuthFlowState`**

In `crates/server/src/state.rs`, add the field to the struct:

```rust
pub struct OAuthFlowState {
    pub provider: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub nonce: String,
    pub pkce_verifier: String,
}
```

- [ ] **Step 2: Update the PKCE imports in `oauth.rs`**

Extend the existing `oauth2` import line to add the PKCE types. Change:

```rust
use oauth2::{AuthorizationCode, CsrfToken, Scope, TokenResponse};
```

to:

```rust
use oauth2::{AuthorizationCode, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, Scope, TokenResponse};
```

- [ ] **Step 3: Generate the PKCE challenge in `oauth_authorize` and store the verifier**

In `oauth_authorize`, change the `auth_request` creation to attach the PKCE
challenge, and generate the verifier just before it. Replace:

```rust
    let mut auth_request = client.authorize_url(CsrfToken::new_random);
```

with:

```rust
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let mut auth_request = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(pkce_challenge);
```

Then add the verifier to the stored struct. Change the `OAuthFlowState { ... }`
literal in `oauth_authorize` to:

```rust
        OAuthFlowState {
            provider,
            created_at: Utc::now(),
            nonce: nonce.clone(),
            pkce_verifier: pkce_verifier.secret().clone(),
        },
```

- [ ] **Step 4: Send the PKCE verifier during token exchange in `oauth_callback`**

Capture the validated flow state (it was discarded in Task 1). Change:

```rust
    validate_and_consume_state(
        &state.oauth_states,
        &query.state,
        &provider,
        nonce_cookie.as_deref(),
    )?;
```

to:

```rust
    let flow = validate_and_consume_state(
        &state.oauth_states,
        &query.state,
        &provider,
        nonce_cookie.as_deref(),
    )?;
```

Then change the code exchange. Replace:

```rust
    let token_result = client
        .exchange_code(AuthorizationCode::new(query.code))
        .request_async(async_http_client)
        .await
        .map_err(|e| AppError::Internal(format!("OAuth token exchange failed: {e}")))?;
```

with:

```rust
    let token_result = client
        .exchange_code(AuthorizationCode::new(query.code))
        .set_pkce_verifier(PkceCodeVerifier::new(flow.pkce_verifier))
        .request_async(async_http_client)
        .await
        .map_err(|e| AppError::Internal(format!("OAuth token exchange failed: {e}")))?;
```

- [ ] **Step 5: Update the test helper to populate `pkce_verifier`**

In the `#[cfg(test)] mod tests` `make_states` helper, add the field to the
struct literal so it compiles:

```rust
            OAuthFlowState {
                provider: provider.to_string(),
                created_at: Utc::now() - chrono::Duration::minutes(age_min),
                nonce: nonce.to_string(),
                pkce_verifier: "verifier1".to_string(),
            },
```

- [ ] **Step 6: Run the tests (still GREEN)**

Run: `cargo test -p serverbee-server --lib router::api::oauth`
Expected: 6 tests pass.

- [ ] **Step 7: Build and lint**

Run: `cargo clippy -p serverbee-server -- -D warnings`
Expected: no warnings, no errors.

- [ ] **Step 8: Commit**

```bash
git add crates/server/src/state.rs crates/server/src/router/api/oauth.rs
git commit -m "feat(server): adopt PKCE (S256) in OAuth login flow"
```

---

## Task 3: Final verification

- [ ] **Step 1: Full workspace build + clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: clean.

- [ ] **Step 2: Run the server test suite**

Run: `cargo test -p serverbee-server`
Expected: all tests pass (including the 6 new oauth tests).

- [ ] **Step 3: Manual verification note**

The full network round-trip (provider token exchange + userinfo) is not unit
tested. If an OAuth provider is configured locally, manually confirm:
1. Clicking "Sign in with GitHub/Google" still completes login.
2. Replaying a captured `callback?code=...&state=...` URL in a *different*
   browser (no `oauth_nonce` cookie) returns HTTP 400 "OAuth session mismatch".
3. The provider request now carries `code_challenge` / `code_challenge_method=S256`.

---

## Self-Review

- **Spec coverage:** browser-binding nonce (Task 1), PKCE (Task 2), single-use
  state (Task 1 test `accepts_valid_state_and_is_single_use`), cookie clearing
  (Task 1 Step 6), constant-time compare (`ct_eq`), no migration/config/frontend
  change — all covered.
- **Placeholder scan:** none — every step shows full code/commands.
- **Type consistency:** `OAuthFlowState` fields (`provider`, `created_at`,
  `nonce`, then `pkce_verifier` in Task 2) match across state.rs, the validator,
  the handlers, and the test helper. `validate_and_consume_state` signature is
  identical everywhere it appears.
