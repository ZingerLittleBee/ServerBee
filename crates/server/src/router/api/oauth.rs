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
use oauth2::{AuthorizationCode, CsrfToken, Scope, TokenResponse};
use sea_orm::EntityTrait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;
use crate::service::auth::AuthService;
use crate::service::oauth::OAuthService;
use crate::state::AppState;

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
) -> Result<Redirect, AppError> {
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

    // Store CSRF state → provider mapping with 10-minute TTL
    state
        .oauth_states
        .insert(csrf_token.secret().clone(), (provider, Utc::now()));

    // Evict expired states (older than 10 minutes) to prevent memory leak
    let cutoff = Utc::now() - chrono::Duration::minutes(10);
    state
        .oauth_states
        .retain(|_, (_, created)| *created > cutoff);

    Ok(Redirect::temporary(auth_url.as_str()))
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
    // Validate CSRF state token
    let stored = state.oauth_states.remove(&query.state);
    match stored {
        Some((_, (stored_provider, created_at))) => {
            // Check provider matches
            if stored_provider != provider {
                return Err(AppError::BadRequest("OAuth state mismatch".to_string()));
            }
            // Check not expired (10 minute window)
            if Utc::now() - created_at > chrono::Duration::minutes(10) {
                return Err(AppError::BadRequest("OAuth state expired".to_string()));
            }
        }
        None => {
            return Err(AppError::BadRequest(
                "Invalid or expired OAuth state".to_string(),
            ));
        }
    }

    let client = OAuthService::build_client(&provider, &state.config.oauth)?;

    // Exchange authorization code for access token
    let token_result = client
        .exchange_code(AuthorizationCode::new(query.code))
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

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        SET_COOKIE,
        cookie
            .parse()
            .map_err(|_| AppError::Internal("Failed to set cookie".to_string()))?,
    );

    Ok((response_headers, Redirect::temporary("/")))
}
