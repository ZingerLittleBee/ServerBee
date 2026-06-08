use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::HeaderMap;
use axum::http::header::SET_COOKIE;
use axum::response::Redirect;
use axum::routing::get;

use crate::router::utils::extract_client_ip;
use chrono::Utc;
use oauth2::reqwest::async_http_client;
use oauth2::{AuthorizationCode, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, Scope, TokenResponse};
use sea_orm::EntityTrait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;
use crate::service::auth::AuthService;
use crate::service::oauth::OAuthService;
use crate::state::{AppState, OAuthFlowState};
use dashmap::DashMap;

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    code: String,
    state: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OAuthProvidersResponse {
    pub providers: Vec<String>,
}

/// OAuth routes (public, no auth required).
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/oauth/providers", get(list_providers))
        .route("/auth/oauth/{provider}", get(oauth_authorize))
        .route("/auth/oauth/{provider}/callback", get(oauth_callback))
}

/// List configured OAuth providers.
#[utoipa::path(
    get,
    path = "/api/auth/oauth/providers",
    tag = "oauth",
    responses(
        (status = 200, description = "List of configured OAuth providers", body = OAuthProvidersResponse),
    )
)]
pub async fn list_providers(
    State(state): State<Arc<AppState>>,
) -> axum::Json<crate::error::ApiResponse<OAuthProvidersResponse>> {
    let mut providers = Vec::new();
    if state.config.oauth.github.is_some() {
        providers.push("github".to_string());
    }
    if state.config.oauth.google.is_some() {
        providers.push("google".to_string());
    }
    // OIDC omitted: userinfo not yet implemented
    axum::Json(crate::error::ApiResponse {
        data: OAuthProvidersResponse { providers },
    })
}

/// Name of the short-lived HttpOnly pre-auth cookie that binds an OAuth login
/// flow to the browser that started it.
const OAUTH_NONCE_COOKIE: &str = "oauth_nonce";

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

/// Redirect user to the OAuth provider's authorization page.
#[utoipa::path(
    get,
    path = "/api/auth/oauth/{provider}",
    tag = "oauth",
    params(("provider" = String, Path, description = "OAuth provider (github, google)")),
    responses(
        (status = 302, description = "Redirect to provider"),
        (status = 400, description = "Provider not configured"),
    )
)]
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

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let mut auth_request = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(pkce_challenge);

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
            pkce_verifier: pkce_verifier.secret().clone(),
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

/// Handle the OAuth callback from the provider.
#[utoipa::path(
    get,
    path = "/api/auth/oauth/{provider}/callback",
    tag = "oauth",
    params(
        ("provider" = String, Path, description = "OAuth provider"),
        ("code" = String, Query, description = "Authorization code"),
        ("state" = String, Query, description = "CSRF state token"),
    ),
    responses(
        (status = 302, description = "Redirect to frontend after login"),
        (status = 400, description = "Invalid callback"),
    )
)]
pub async fn oauth_callback(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(provider): Path<String>,
    Query(query): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Result<(HeaderMap, Redirect), AppError> {
    // Validate CSRF state + browser-binding nonce, atomically consuming the state.
    let nonce_cookie = extract_oauth_nonce(&headers);
    let flow = validate_and_consume_state(
        &state.oauth_states,
        &query.state,
        &provider,
        nonce_cookie.as_deref(),
    )?;

    let client = OAuthService::build_client(&provider, &state.config.oauth)?;

    // Exchange authorization code for access token, supplying the PKCE verifier
    // to prove this request originates from the same party that initiated the flow.
    let token_result = client
        .exchange_code(AuthorizationCode::new(query.code))
        .set_pkce_verifier(PkceCodeVerifier::new(flow.pkce_verifier))
        .request_async(async_http_client)
        .await
        .map_err(|e| AppError::Internal(format!("OAuth token exchange failed: {e}")))?;

    let access_token = token_result.access_token().secret();

    // Fetch user info from provider
    let user_info = OAuthService::fetch_user_info(&provider, access_token).await?;

    // Find or create the local user
    let user = OAuthService::find_or_create_user(
        &state.db,
        &provider,
        &user_info,
        state.config.oauth.allow_registration,
    )
    .await?;

    // Extract real IP and user-agent from request headers
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Create a session
    let token = AuthService::generate_session_token();
    let now = Utc::now();
    let expires_at = now + chrono::Duration::seconds(state.config.auth.session_ttl);

    let new_session = crate::entity::session::ActiveModel {
        id: sea_orm::Set(Uuid::new_v4().to_string()),
        user_id: sea_orm::Set(user.id.clone()),
        token: sea_orm::Set(token.clone()),
        ip: sea_orm::Set(ip),
        user_agent: sea_orm::Set(user_agent),
        expires_at: sea_orm::Set(expires_at),
        created_at: sea_orm::Set(now),
        source: sea_orm::Set("web".to_string()),
        mobile_session_id: sea_orm::Set(None),
    };

    crate::entity::session::Entity::insert(new_session)
        .exec(&state.db)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create session: {e}")))?;

    // Set session cookie and redirect to frontend
    // Use SameSite=Lax because redirect comes from external provider
    let secure_flag = if state.config.auth.secure_cookie {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "session_token={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}{}",
        token, state.config.auth.session_ttl, secure_flag
    );

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
}

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
                pkce_verifier: "verifier1".to_string(),
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
        // the PKCE verifier must round-trip back to the caller for token exchange
        assert_eq!(flow.pkce_verifier, "verifier1");
        // second use must fail: state was consumed (replay protection)
        let err =
            validate_and_consume_state(&states, "s1", "github", Some("nonce1")).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }
}
