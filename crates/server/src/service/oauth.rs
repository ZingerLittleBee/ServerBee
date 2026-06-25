use chrono::Utc;
use oauth2::basic::BasicClient;
use oauth2::{AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use sea_orm::*;
use uuid::Uuid;

use crate::config::{OAuthConfig, OAuthProviderConfig};
use crate::entity::{oauth_account, user};
use crate::error::AppError;
use crate::service::auth::AuthService;

/// Supported OAuth provider names.
pub const PROVIDER_GITHUB: &str = "github";
pub const PROVIDER_GOOGLE: &str = "google";
pub const PROVIDER_OIDC: &str = "oidc";

/// User info fetched from the OAuth provider.
pub struct OAuthUserInfo {
    pub provider_user_id: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
}

pub struct OAuthService;

impl OAuthService {
    /// Build an oauth2 BasicClient for the given provider.
    pub fn build_client(provider: &str, config: &OAuthConfig) -> Result<BasicClient, AppError> {
        let base_url = config.base_url.trim_end_matches('/');
        let redirect_url = format!("{base_url}/api/auth/oauth/{provider}/callback");

        match provider {
            PROVIDER_GITHUB => {
                let p = config.github.as_ref().ok_or_else(|| {
                    AppError::BadRequest("GitHub OAuth not configured".to_string())
                })?;
                Ok(build_basic_client(
                    p,
                    "https://github.com/login/oauth/authorize",
                    "https://github.com/login/oauth/access_token",
                    &redirect_url,
                )?)
            }
            PROVIDER_GOOGLE => {
                let p = config.google.as_ref().ok_or_else(|| {
                    AppError::BadRequest("Google OAuth not configured".to_string())
                })?;
                Ok(build_basic_client(
                    p,
                    "https://accounts.google.com/o/oauth2/v2/auth",
                    "https://oauth2.googleapis.com/token",
                    &redirect_url,
                )?)
            }
            PROVIDER_OIDC => {
                let p = config
                    .oidc
                    .as_ref()
                    .ok_or_else(|| AppError::BadRequest("OIDC OAuth not configured".to_string()))?;
                let auth_url = format!("{}/authorize", p.issuer_url.trim_end_matches('/'));
                let token_url = format!("{}/oauth/token", p.issuer_url.trim_end_matches('/'));
                Ok(build_basic_client_raw(
                    &p.client_id,
                    &p.client_secret,
                    &auth_url,
                    &token_url,
                    &redirect_url,
                )?)
            }
            _ => Err(AppError::BadRequest(format!(
                "Unknown OAuth provider: {provider}"
            ))),
        }
    }

    /// Check if a provider is configured.
    pub fn is_configured(provider: &str, config: &OAuthConfig) -> bool {
        match provider {
            PROVIDER_GITHUB => config.github.is_some(),
            PROVIDER_GOOGLE => config.google.is_some(),
            PROVIDER_OIDC => config.oidc.is_some(),
            _ => false,
        }
    }

    /// Get the list of configured providers.
    pub fn configured_providers(config: &OAuthConfig) -> Vec<String> {
        let mut providers = Vec::new();
        if config.github.is_some() {
            providers.push(PROVIDER_GITHUB.to_string());
        }
        if config.google.is_some() {
            providers.push(PROVIDER_GOOGLE.to_string());
        }
        if config.oidc.is_some() {
            providers.push(PROVIDER_OIDC.to_string());
        }
        providers
    }

    /// Fetch user info from the provider using the access token.
    pub async fn fetch_user_info(
        provider: &str,
        access_token: &str,
    ) -> Result<OAuthUserInfo, AppError> {
        let client = reqwest::Client::new();

        match provider {
            PROVIDER_GITHUB => {
                let resp: serde_json::Value = client
                    .get("https://api.github.com/user")
                    .header("Authorization", format!("Bearer {access_token}"))
                    .header("User-Agent", "ServerBee")
                    .send()
                    .await
                    .map_err(|e| AppError::Internal(format!("GitHub API error: {e}")))?
                    .json()
                    .await
                    .map_err(|e| AppError::Internal(format!("GitHub API parse error: {e}")))?;

                Ok(OAuthUserInfo {
                    provider_user_id: resp["id"].as_i64().unwrap_or(0).to_string(),
                    email: resp["email"].as_str().map(|s| s.to_string()),
                    display_name: resp["login"]
                        .as_str()
                        .or(resp["name"].as_str())
                        .map(|s| s.to_string()),
                })
            }
            PROVIDER_GOOGLE => {
                let resp: serde_json::Value = client
                    .get("https://www.googleapis.com/oauth2/v3/userinfo")
                    .header("Authorization", format!("Bearer {access_token}"))
                    .send()
                    .await
                    .map_err(|e| AppError::Internal(format!("Google API error: {e}")))?
                    .json()
                    .await
                    .map_err(|e| AppError::Internal(format!("Google API parse error: {e}")))?;

                Ok(OAuthUserInfo {
                    provider_user_id: resp["sub"].as_str().unwrap_or_default().to_string(),
                    email: resp["email"].as_str().map(|s| s.to_string()),
                    display_name: resp["name"].as_str().map(|s| s.to_string()),
                })
            }
            PROVIDER_OIDC => {
                // Generic: use Bearer token on a userinfo endpoint
                // The caller should configure the OIDC userinfo endpoint
                // For now, we try the standard endpoint pattern
                Err(AppError::Internal(
                    "Generic OIDC userinfo not yet implemented".to_string(),
                ))
            }
            _ => Err(AppError::BadRequest(format!(
                "Unknown provider: {provider}"
            ))),
        }
    }

    /// Find or create a user for the given OAuth identity.
    /// If an oauth_account with this provider+provider_user_id exists, return the linked user.
    /// Otherwise, create a new user (with a random password) and link the OAuth account.
    /// If `allow_registration` is false, only existing linked accounts are allowed.
    pub async fn find_or_create_user(
        db: &DatabaseConnection,
        provider: &str,
        info: &OAuthUserInfo,
        allow_registration: bool,
    ) -> Result<user::Model, AppError> {
        // Check if OAuth account already exists
        let existing = oauth_account::Entity::find()
            .filter(oauth_account::Column::Provider.eq(provider))
            .filter(oauth_account::Column::ProviderUserId.eq(&info.provider_user_id))
            .one(db)
            .await?;

        if let Some(account) = existing {
            // Return the linked user
            let user = user::Entity::find_by_id(&account.user_id)
                .one(db)
                .await?
                .ok_or_else(|| AppError::Internal("OAuth-linked user not found".to_string()))?;
            return Ok(user);
        }

        // Reject new registrations if not allowed
        if !allow_registration {
            return Err(AppError::BadRequest(
                "OAuth registration is disabled. Ask an admin to link your account.".to_string(),
            ));
        }

        // Create a new user
        let username = generate_oauth_username(db, provider, info).await;
        let random_password = AuthService::generate_session_token();
        let user = AuthService::create_user(db, &username, &random_password, "member").await?;

        // Link OAuth account
        let now = Utc::now();
        let account = oauth_account::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            user_id: Set(user.id.clone()),
            provider: Set(provider.to_string()),
            provider_user_id: Set(info.provider_user_id.clone()),
            email: Set(info.email.clone()),
            display_name: Set(info.display_name.clone()),
            created_at: Set(now),
        };
        account.insert(db).await?;

        Ok(user)
    }

    /// Link an OAuth account to an existing user.
    pub async fn link_account(
        db: &DatabaseConnection,
        user_id: &str,
        provider: &str,
        info: &OAuthUserInfo,
    ) -> Result<oauth_account::Model, AppError> {
        // Check if already linked by another user
        let existing = oauth_account::Entity::find()
            .filter(oauth_account::Column::Provider.eq(provider))
            .filter(oauth_account::Column::ProviderUserId.eq(&info.provider_user_id))
            .one(db)
            .await?;

        if let Some(acc) = existing {
            if acc.user_id == user_id {
                return Ok(acc);
            }
            return Err(AppError::Conflict(
                "This OAuth account is already linked to another user".to_string(),
            ));
        }

        let now = Utc::now();
        let account = oauth_account::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            user_id: Set(user_id.to_string()),
            provider: Set(provider.to_string()),
            provider_user_id: Set(info.provider_user_id.clone()),
            email: Set(info.email.clone()),
            display_name: Set(info.display_name.clone()),
            created_at: Set(now),
        };
        Ok(account.insert(db).await?)
    }

    /// List OAuth accounts for a user.
    pub async fn list_accounts(
        db: &DatabaseConnection,
        user_id: &str,
    ) -> Result<Vec<oauth_account::Model>, AppError> {
        Ok(oauth_account::Entity::find()
            .filter(oauth_account::Column::UserId.eq(user_id))
            .all(db)
            .await?)
    }

    /// Unlink an OAuth account.
    pub async fn unlink_account(
        db: &DatabaseConnection,
        id: &str,
        user_id: &str,
    ) -> Result<(), AppError> {
        let result = oauth_account::Entity::delete_many()
            .filter(oauth_account::Column::Id.eq(id))
            .filter(oauth_account::Column::UserId.eq(user_id))
            .exec(db)
            .await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound("OAuth account not found".to_string()));
        }
        Ok(())
    }
}

fn build_basic_client(
    config: &OAuthProviderConfig,
    auth_url: &str,
    token_url: &str,
    redirect_url: &str,
) -> Result<BasicClient, AppError> {
    build_basic_client_raw(
        &config.client_id,
        &config.client_secret,
        auth_url,
        token_url,
        redirect_url,
    )
}

fn build_basic_client_raw(
    client_id: &str,
    client_secret: &str,
    auth_url: &str,
    token_url: &str,
    redirect_url: &str,
) -> Result<BasicClient, AppError> {
    Ok(BasicClient::new(
        ClientId::new(client_id.to_string()),
        Some(ClientSecret::new(client_secret.to_string())),
        AuthUrl::new(auth_url.to_string())
            .map_err(|e| AppError::Internal(format!("Invalid auth URL: {e}")))?,
        Some(
            TokenUrl::new(token_url.to_string())
                .map_err(|e| AppError::Internal(format!("Invalid token URL: {e}")))?,
        ),
    )
    .set_redirect_uri(
        RedirectUrl::new(redirect_url.to_string())
            .map_err(|e| AppError::Internal(format!("Invalid redirect URL: {e}")))?,
    ))
}

/// Generate a unique username for an OAuth user.
async fn generate_oauth_username(
    db: &DatabaseConnection,
    provider: &str,
    info: &OAuthUserInfo,
) -> String {
    // Try display_name first, then email prefix, then provider_user_id
    let base = info
        .display_name
        .as_deref()
        .or(info
            .email
            .as_deref()
            .map(|e| e.split('@').next().unwrap_or("user")))
        .unwrap_or("user");

    let candidate = format!("{provider}_{base}");

    // Check if exists, append random suffix if needed
    let exists = user::Entity::find()
        .filter(user::Column::Username.eq(&candidate))
        .one(db)
        .await
        .ok()
        .flatten();

    if exists.is_none() {
        return candidate;
    }

    // Append random suffix
    format!("{candidate}_{}", &Uuid::new_v4().to_string()[..6])
}

#[cfg(test)]
mod tests {
    use super::*;
    // OAuthConfig/OAuthProviderConfig/oauth_account/user are re-exported via `super::*`;
    // only OIDCProviderConfig needs an explicit import here.
    use crate::config::OIDCProviderConfig;
    use crate::test_utils::setup_test_db;
    use oauth2::CsrfToken;

    // Build an OAuthConfig with the requested providers enabled.
    fn config_with(
        github: bool,
        google: bool,
        oidc: Option<&str>,
        base_url: &str,
    ) -> OAuthConfig {
        OAuthConfig {
            github: github.then(|| OAuthProviderConfig {
                client_id: "gh-client".to_string(),
                client_secret: "gh-secret".to_string(),
            }),
            google: google.then(|| OAuthProviderConfig {
                client_id: "goog-client".to_string(),
                client_secret: "goog-secret".to_string(),
            }),
            oidc: oidc.map(|issuer| OIDCProviderConfig {
                issuer_url: issuer.to_string(),
                client_id: "oidc-client".to_string(),
                client_secret: "oidc-secret".to_string(),
                scopes: vec!["openid".to_string()],
            }),
            base_url: base_url.to_string(),
            allow_registration: false,
        }
    }

    fn user_info(id: &str, email: Option<&str>, name: Option<&str>) -> OAuthUserInfo {
        OAuthUserInfo {
            provider_user_id: id.to_string(),
            email: email.map(|s| s.to_string()),
            display_name: name.map(|s| s.to_string()),
        }
    }

    // build_client(github): produces a client whose authorize URL and redirect URL
    // are derived from GitHub's endpoints and the configured base_url.
    #[test]
    fn build_client_github_authorize_and_redirect_urls() {
        let cfg = config_with(true, false, None, "https://example.com");
        let client = OAuthService::build_client(PROVIDER_GITHUB, &cfg).expect("github client");

        // Redirect URL is built from base_url + provider callback path.
        assert_eq!(
            client.redirect_url().expect("redirect url set").url().as_str(),
            "https://example.com/api/auth/oauth/github/callback"
        );

        // Authorize URL points at GitHub and carries the configured client_id + a CSRF state.
        let (auth_url, csrf) = client
            .authorize_url(|| CsrfToken::new("fixed-state".to_string()))
            .url();
        assert_eq!(auth_url.scheme(), "https");
        assert_eq!(auth_url.host_str(), Some("github.com"));
        assert_eq!(auth_url.path(), "/login/oauth/authorize");
        let q: std::collections::HashMap<_, _> = auth_url.query_pairs().into_owned().collect();
        assert_eq!(q.get("client_id").map(String::as_str), Some("gh-client"));
        assert_eq!(q.get("state").map(String::as_str), Some("fixed-state"));
        assert_eq!(q.get("response_type").map(String::as_str), Some("code"));
        assert_eq!(csrf.secret(), "fixed-state");
    }

    // build_client trims a trailing slash on base_url before composing the callback URL.
    #[test]
    fn build_client_trims_trailing_slash_in_base_url() {
        let cfg = config_with(true, false, None, "https://example.com/");
        let client = OAuthService::build_client(PROVIDER_GITHUB, &cfg).expect("github client");
        assert_eq!(
            client.redirect_url().expect("redirect url set").url().as_str(),
            "https://example.com/api/auth/oauth/github/callback"
        );
    }

    // build_client(google): points the authorize URL at Google's accounts endpoint.
    #[test]
    fn build_client_google_uses_google_endpoints() {
        let cfg = config_with(false, true, None, "https://srv.test");
        let client = OAuthService::build_client(PROVIDER_GOOGLE, &cfg).expect("google client");
        let (auth_url, _) = client
            .authorize_url(|| CsrfToken::new("s".to_string()))
            .url();
        assert_eq!(auth_url.host_str(), Some("accounts.google.com"));
        assert_eq!(auth_url.path(), "/o/oauth2/v2/auth");
        assert_eq!(
            client.redirect_url().expect("redirect url set").url().as_str(),
            "https://srv.test/api/auth/oauth/google/callback"
        );
    }

    // build_client(oidc): derives authorize/token URLs from the issuer_url (trailing slash trimmed).
    #[test]
    fn build_client_oidc_derives_urls_from_issuer() {
        let cfg = config_with(false, false, Some("https://idp.test/"), "https://srv.test");
        let client = OAuthService::build_client(PROVIDER_OIDC, &cfg).expect("oidc client");
        let (auth_url, _) = client
            .authorize_url(|| CsrfToken::new("s".to_string()))
            .url();
        assert_eq!(auth_url.host_str(), Some("idp.test"));
        assert_eq!(auth_url.path(), "/authorize");
        let q: std::collections::HashMap<_, _> = auth_url.query_pairs().into_owned().collect();
        assert_eq!(q.get("client_id").map(String::as_str), Some("oidc-client"));
    }

    // build_client errors with BadRequest when the requested provider is not configured.
    #[test]
    fn build_client_github_not_configured() {
        let cfg = config_with(false, false, None, "https://srv.test");
        let err = OAuthService::build_client(PROVIDER_GITHUB, &cfg)
            .err()
            .expect("should error when github unconfigured");
        match err {
            AppError::BadRequest(msg) => assert!(msg.contains("GitHub OAuth not configured")),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    // build_client errors when google is requested but not configured.
    #[test]
    fn build_client_google_not_configured() {
        let cfg = config_with(false, false, None, "https://srv.test");
        let err = OAuthService::build_client(PROVIDER_GOOGLE, &cfg)
            .err()
            .expect("should error when google unconfigured");
        match err {
            AppError::BadRequest(msg) => assert!(msg.contains("Google OAuth not configured")),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    // build_client errors when oidc is requested but not configured.
    #[test]
    fn build_client_oidc_not_configured() {
        let cfg = config_with(false, false, None, "https://srv.test");
        let err = OAuthService::build_client(PROVIDER_OIDC, &cfg)
            .err()
            .expect("should error when oidc unconfigured");
        match err {
            AppError::BadRequest(msg) => assert!(msg.contains("OIDC OAuth not configured")),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    // build_client rejects an unknown provider name with a descriptive BadRequest.
    #[test]
    fn build_client_unknown_provider() {
        let cfg = config_with(true, true, Some("https://idp.test"), "https://srv.test");
        let err = OAuthService::build_client("facebook", &cfg)
            .err()
            .expect("should error on unknown provider");
        match err {
            AppError::BadRequest(msg) => {
                assert!(msg.contains("Unknown OAuth provider: facebook"))
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    // build_client surfaces an Internal error when the OIDC issuer yields an invalid auth URL.
    #[test]
    fn build_client_oidc_invalid_issuer_url() {
        let cfg = config_with(false, false, Some("not a url"), "https://srv.test");
        let err = OAuthService::build_client(PROVIDER_OIDC, &cfg)
            .err()
            .expect("should error on invalid issuer URL");
        match err {
            AppError::Internal(msg) => assert!(msg.contains("Invalid auth URL")),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    // is_configured returns true only for the providers actually present in config.
    #[test]
    fn is_configured_matches_present_providers() {
        let cfg = config_with(true, false, Some("https://idp.test"), "https://srv.test");
        assert!(OAuthService::is_configured(PROVIDER_GITHUB, &cfg));
        assert!(!OAuthService::is_configured(PROVIDER_GOOGLE, &cfg));
        assert!(OAuthService::is_configured(PROVIDER_OIDC, &cfg));
        // Unknown provider is never configured.
        assert!(!OAuthService::is_configured("twitter", &cfg));
    }

    // is_configured returns false for every provider when none is configured.
    #[test]
    fn is_configured_none_when_empty() {
        let cfg = OAuthConfig::default();
        assert!(!OAuthService::is_configured(PROVIDER_GITHUB, &cfg));
        assert!(!OAuthService::is_configured(PROVIDER_GOOGLE, &cfg));
        assert!(!OAuthService::is_configured(PROVIDER_OIDC, &cfg));
    }

    // configured_providers lists all enabled providers in github/google/oidc order.
    #[test]
    fn configured_providers_lists_all_in_order() {
        let cfg = config_with(true, true, Some("https://idp.test"), "https://srv.test");
        assert_eq!(
            OAuthService::configured_providers(&cfg),
            vec![
                PROVIDER_GITHUB.to_string(),
                PROVIDER_GOOGLE.to_string(),
                PROVIDER_OIDC.to_string(),
            ]
        );
    }

    // configured_providers returns an empty vec when nothing is configured.
    #[test]
    fn configured_providers_empty_when_none() {
        assert!(OAuthService::configured_providers(&OAuthConfig::default()).is_empty());
    }

    // configured_providers returns only the configured subset (skips disabled github).
    #[test]
    fn configured_providers_subset() {
        let cfg = config_with(false, true, None, "https://srv.test");
        assert_eq!(
            OAuthService::configured_providers(&cfg),
            vec![PROVIDER_GOOGLE.to_string()]
        );
    }

    // fetch_user_info for the OIDC provider returns the "not yet implemented" Internal error
    // without making any network call.
    #[tokio::test]
    async fn fetch_user_info_oidc_not_implemented() {
        let err = OAuthService::fetch_user_info(PROVIDER_OIDC, "tok")
            .await
            .err()
            .expect("oidc userinfo should be unimplemented");
        match err {
            AppError::Internal(msg) => assert!(msg.contains("Generic OIDC userinfo not yet implemented")),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    // fetch_user_info rejects an unknown provider with BadRequest, no network call.
    #[tokio::test]
    async fn fetch_user_info_unknown_provider() {
        let err = OAuthService::fetch_user_info("linkedin", "tok")
            .await
            .err()
            .expect("unknown provider should error");
        match err {
            AppError::BadRequest(msg) => assert!(msg.contains("Unknown provider: linkedin")),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    // find_or_create_user returns the already-linked user when an oauth_account exists.
    #[tokio::test]
    async fn find_or_create_user_returns_existing_linked_user() {
        let (db, _tmp) = setup_test_db().await;
        let existing = AuthService::create_user(&db, "alice", "password123", "member")
            .await
            .unwrap();
        // Pre-link an oauth account for github / external id 42.
        oauth_account::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            user_id: Set(existing.id.clone()),
            provider: Set(PROVIDER_GITHUB.to_string()),
            provider_user_id: Set("42".to_string()),
            email: Set(None),
            display_name: Set(None),
            created_at: Set(Utc::now()),
        }
        .insert(&db)
        .await
        .unwrap();

        let info = user_info("42", Some("a@x.com"), Some("Alice"));
        // allow_registration=false but the account is already linked, so it succeeds.
        let user = OAuthService::find_or_create_user(&db, PROVIDER_GITHUB, &info, false)
            .await
            .expect("linked user found");
        assert_eq!(user.id, existing.id);
        assert_eq!(user.username, "alice");
        // No new user was created.
        assert_eq!(user::Entity::find().count(&db).await.unwrap(), 1);
    }

    // find_or_create_user surfaces an Internal error when a linked account points at a missing user.
    #[tokio::test]
    async fn find_or_create_user_orphan_account_errors() {
        use sea_orm::ConnectionTrait;
        let (db, _tmp) = setup_test_db().await;
        // The FK on oauth_accounts.user_id is ON DELETE CASCADE, so a real
        // orphan cannot arise through normal operations — the branch under test
        // is defensive. Manufacture it: create a real linked account, then drop
        // the parent user with FK enforcement temporarily off so the cascade
        // does not also remove the account.
        let ghost = AuthService::create_user(&db, "ghost", "password123", "member")
            .await
            .unwrap();
        oauth_account::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            user_id: Set(ghost.id.clone()),
            provider: Set(PROVIDER_GITHUB.to_string()),
            provider_user_id: Set("99".to_string()),
            email: Set(None),
            display_name: Set(None),
            created_at: Set(Utc::now()),
        }
        .insert(&db)
        .await
        .unwrap();
        db.execute_unprepared(&format!(
            "PRAGMA foreign_keys=OFF; DELETE FROM users WHERE id='{}'; PRAGMA foreign_keys=ON;",
            ghost.id
        ))
        .await
        .unwrap();

        let info = user_info("99", None, None);
        let err = OAuthService::find_or_create_user(&db, PROVIDER_GITHUB, &info, true)
            .await
            .err()
            .expect("orphan account should error");
        match err {
            AppError::Internal(msg) => assert!(msg.contains("OAuth-linked user not found")),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    // find_or_create_user rejects an unlinked identity when registration is disabled.
    #[tokio::test]
    async fn find_or_create_user_registration_disabled() {
        let (db, _tmp) = setup_test_db().await;
        let info = user_info("123", Some("new@x.com"), Some("New"));
        let err = OAuthService::find_or_create_user(&db, PROVIDER_GITHUB, &info, false)
            .await
            .err()
            .expect("registration disabled should error");
        match err {
            AppError::BadRequest(msg) => assert!(msg.contains("OAuth registration is disabled")),
            other => panic!("expected BadRequest, got {other:?}"),
        }
        // No user or account was created.
        assert_eq!(user::Entity::find().count(&db).await.unwrap(), 0);
        assert_eq!(oauth_account::Entity::find().count(&db).await.unwrap(), 0);
    }

    // find_or_create_user creates a new user + linked oauth account when registration is allowed.
    // The username is derived from the display_name ("github_NewGuy").
    #[tokio::test]
    async fn find_or_create_user_creates_and_links_new_user() {
        let (db, _tmp) = setup_test_db().await;
        let info = user_info("555", Some("guy@x.com"), Some("NewGuy"));
        let user = OAuthService::find_or_create_user(&db, PROVIDER_GITHUB, &info, true)
            .await
            .expect("new user created");
        assert_eq!(user.username, "github_NewGuy");
        assert_eq!(user.role, "member");

        // A linked oauth account row now exists carrying the provider identity + profile.
        let acc = oauth_account::Entity::find()
            .filter(oauth_account::Column::ProviderUserId.eq("555"))
            .one(&db)
            .await
            .unwrap()
            .expect("oauth account linked");
        assert_eq!(acc.user_id, user.id);
        assert_eq!(acc.provider, PROVIDER_GITHUB);
        assert_eq!(acc.email.as_deref(), Some("guy@x.com"));
        assert_eq!(acc.display_name.as_deref(), Some("NewGuy"));
    }

    // generate_oauth_username (via find_or_create_user) falls back to the email prefix
    // when display_name is absent.
    #[tokio::test]
    async fn new_user_username_uses_email_prefix_when_no_display_name() {
        let (db, _tmp) = setup_test_db().await;
        let info = user_info("777", Some("john.doe@example.com"), None);
        let user = OAuthService::find_or_create_user(&db, PROVIDER_GOOGLE, &info, true)
            .await
            .expect("new user created");
        assert_eq!(user.username, "google_john.doe");
    }

    // generate_oauth_username falls back to "user" when neither display_name nor email exist.
    #[tokio::test]
    async fn new_user_username_falls_back_to_user() {
        let (db, _tmp) = setup_test_db().await;
        let info = user_info("888", None, None);
        let user = OAuthService::find_or_create_user(&db, PROVIDER_GITHUB, &info, true)
            .await
            .expect("new user created");
        assert_eq!(user.username, "github_user");
    }

    // generate_oauth_username appends a random suffix when the base username collides.
    #[tokio::test]
    async fn new_user_username_appends_suffix_on_collision() {
        let (db, _tmp) = setup_test_db().await;
        // Pre-create a user occupying the would-be candidate username.
        AuthService::create_user(&db, "github_Dup", "password123", "member")
            .await
            .unwrap();

        let info = user_info("999", None, Some("Dup"));
        let user = OAuthService::find_or_create_user(&db, PROVIDER_GITHUB, &info, true)
            .await
            .expect("new user created");
        // Candidate "github_Dup" collides, so a 6-char hex suffix is appended.
        assert!(user.username.starts_with("github_Dup_"));
        assert_eq!(user.username.len(), "github_Dup_".len() + 6);
        assert_ne!(user.username, "github_Dup");
    }

    // link_account creates a new oauth_account linked to the given user.
    #[tokio::test]
    async fn link_account_creates_new_link() {
        let (db, _tmp) = setup_test_db().await;
        let u = AuthService::create_user(&db, "bob", "password123", "member")
            .await
            .unwrap();
        let info = user_info("g-1", Some("bob@x.com"), Some("Bob"));
        let acc = OAuthService::link_account(&db, &u.id, PROVIDER_GITHUB, &info)
            .await
            .expect("link created");
        assert_eq!(acc.user_id, u.id);
        assert_eq!(acc.provider, PROVIDER_GITHUB);
        assert_eq!(acc.provider_user_id, "g-1");
        assert_eq!(acc.email.as_deref(), Some("bob@x.com"));
        assert_eq!(oauth_account::Entity::find().count(&db).await.unwrap(), 1);
    }

    // link_account is idempotent: re-linking the same identity to the same user returns
    // the existing row without creating a duplicate.
    #[tokio::test]
    async fn link_account_idempotent_for_same_user() {
        let (db, _tmp) = setup_test_db().await;
        let u = AuthService::create_user(&db, "carol", "password123", "member")
            .await
            .unwrap();
        let info = user_info("g-2", None, None);
        let first = OAuthService::link_account(&db, &u.id, PROVIDER_GITHUB, &info)
            .await
            .unwrap();
        let second = OAuthService::link_account(&db, &u.id, PROVIDER_GITHUB, &info)
            .await
            .expect("re-link returns existing");
        assert_eq!(first.id, second.id);
        assert_eq!(oauth_account::Entity::find().count(&db).await.unwrap(), 1);
    }

    // link_account rejects linking an identity already owned by a different user with Conflict.
    #[tokio::test]
    async fn link_account_conflict_for_other_user() {
        let (db, _tmp) = setup_test_db().await;
        let owner = AuthService::create_user(&db, "owner", "password123", "member")
            .await
            .unwrap();
        let other = AuthService::create_user(&db, "other", "password123", "member")
            .await
            .unwrap();
        let info = user_info("g-3", None, None);
        OAuthService::link_account(&db, &owner.id, PROVIDER_GITHUB, &info)
            .await
            .unwrap();

        let err = OAuthService::link_account(&db, &other.id, PROVIDER_GITHUB, &info)
            .await
            .err()
            .expect("linking another user's identity should conflict");
        match err {
            AppError::Conflict(msg) => {
                assert!(msg.contains("already linked to another user"))
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    // list_accounts returns an empty vec for a user with no linked accounts.
    #[tokio::test]
    async fn list_accounts_empty() {
        let (db, _tmp) = setup_test_db().await;
        let u = AuthService::create_user(&db, "dan", "password123", "member")
            .await
            .unwrap();
        let accounts = OAuthService::list_accounts(&db, &u.id).await.unwrap();
        assert!(accounts.is_empty());
    }

    // list_accounts returns only the requesting user's linked accounts.
    #[tokio::test]
    async fn list_accounts_returns_only_users_accounts() {
        let (db, _tmp) = setup_test_db().await;
        let u = AuthService::create_user(&db, "erin", "password123", "member")
            .await
            .unwrap();
        let other = AuthService::create_user(&db, "frank", "password123", "member")
            .await
            .unwrap();
        OAuthService::link_account(&db, &u.id, PROVIDER_GITHUB, &user_info("a", None, None))
            .await
            .unwrap();
        OAuthService::link_account(&db, &u.id, PROVIDER_GOOGLE, &user_info("b", None, None))
            .await
            .unwrap();
        // Account belonging to another user must not appear.
        OAuthService::link_account(&db, &other.id, PROVIDER_OIDC, &user_info("c", None, None))
            .await
            .unwrap();

        let accounts = OAuthService::list_accounts(&db, &u.id).await.unwrap();
        assert_eq!(accounts.len(), 2);
        assert!(accounts.iter().all(|a| a.user_id == u.id));
    }

    // unlink_account deletes the matching account and reports success.
    #[tokio::test]
    async fn unlink_account_success() {
        let (db, _tmp) = setup_test_db().await;
        let u = AuthService::create_user(&db, "gwen", "password123", "member")
            .await
            .unwrap();
        let acc = OAuthService::link_account(&db, &u.id, PROVIDER_GITHUB, &user_info("x", None, None))
            .await
            .unwrap();

        OAuthService::unlink_account(&db, &acc.id, &u.id)
            .await
            .expect("unlink succeeds");
        assert_eq!(oauth_account::Entity::find().count(&db).await.unwrap(), 0);
    }

    // unlink_account returns NotFound when the id/user pair matches no row
    // (e.g. another user's account id).
    #[tokio::test]
    async fn unlink_account_not_found_for_wrong_owner() {
        let (db, _tmp) = setup_test_db().await;
        let owner = AuthService::create_user(&db, "hank", "password123", "member")
            .await
            .unwrap();
        let intruder = AuthService::create_user(&db, "ivan", "password123", "member")
            .await
            .unwrap();
        let acc = OAuthService::link_account(&db, &owner.id, PROVIDER_GITHUB, &user_info("y", None, None))
            .await
            .unwrap();

        // Intruder cannot delete owner's account: no row matches id + intruder.id.
        let err = OAuthService::unlink_account(&db, &acc.id, &intruder.id)
            .await
            .err()
            .expect("wrong owner should not unlink");
        match err {
            AppError::NotFound(msg) => assert!(msg.contains("OAuth account not found")),
            other => panic!("expected NotFound, got {other:?}"),
        }
        // The owner's account is untouched.
        assert_eq!(oauth_account::Entity::find().count(&db).await.unwrap(), 1);
    }

    // unlink_account returns NotFound for a completely unknown id.
    #[tokio::test]
    async fn unlink_account_not_found_for_unknown_id() {
        let (db, _tmp) = setup_test_db().await;
        let u = AuthService::create_user(&db, "jane", "password123", "member")
            .await
            .unwrap();
        let err = OAuthService::unlink_account(&db, "nope", &u.id)
            .await
            .err()
            .expect("unknown id should error");
        match err {
            AppError::NotFound(msg) => assert!(msg.contains("OAuth account not found")),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }
}
