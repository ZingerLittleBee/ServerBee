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
