use chrono::Utc;
use sea_orm::prelude::Expr;
use sea_orm::*;
use uuid::Uuid;

use crate::entity::{mobile_device_registration, mobile_session, user};
use crate::error::AppError;
use crate::service::auth::AuthService;
use crate::service::jwt::JwtService;

pub struct MobileAuthService;

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct MobileTokenResponse {
    pub access_token: String,
    pub access_expires_in_secs: i64,
    pub refresh_token: String,
    pub refresh_expires_in_secs: i64,
    pub token_type: String,
    pub user: MobileUser,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct MobileUser {
    pub id: String,
    pub username: String,
    pub role: String,
}

impl MobileAuthService {
    /// Login: validates credentials, creates JWT access token + DB refresh token.
    #[allow(clippy::too_many_arguments)]
    pub async fn login(
        db: &DatabaseConnection,
        jwt: &JwtService,
        username: &str,
        password: &str,
        totp_code: Option<&str>,
        installation_id: &str,
        refresh_ttl: i64,
        ip: &str,
        user_agent: &str,
    ) -> Result<MobileTokenResponse, AppError> {
        // 1. Validate credentials (reuse existing AuthService)
        let user =
            AuthService::validate_credentials(db, username, password, totp_code).await?;

        // 2. Generate tokens
        let (access_token, access_expires_in_secs) = jwt
            .create_access_token(&user.id, &user.username, &user.role)
            .map_err(|e| AppError::Internal(format!("JWT error: {e}")))?;

        let refresh_token = Self::generate_refresh_token();
        let refresh_hash = AuthService::hash_token(&refresh_token);

        // 3. Revoke any existing session for this installation
        Self::revoke_by_installation(db, installation_id).await?;

        let now = Utc::now();
        let session = mobile_session::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            user_id: Set(user.id.clone()),
            installation_id: Set(installation_id.to_string()),
            refresh_token_hash: Set(refresh_hash),
            revoked_at: Set(None),
            ip: Set(ip.to_string()),
            user_agent: Set(user_agent.to_string()),
            expires_at: Set(now + chrono::Duration::seconds(refresh_ttl)),
            created_at: Set(now),
            updated_at: Set(now),
        };
        mobile_session::Entity::insert(session).exec(db).await?;

        Ok(MobileTokenResponse {
            access_token,
            access_expires_in_secs,
            refresh_token,
            refresh_expires_in_secs: refresh_ttl,
            token_type: "Bearer".to_string(),
            user: MobileUser {
                id: user.id.clone(),
                username: user.username.clone(),
                role: user.role.clone(),
            },
        })
    }

    /// Refresh: validates refresh token, rotates to new pair.
    pub async fn refresh(
        db: &DatabaseConnection,
        jwt: &JwtService,
        refresh_token: &str,
        installation_id: &str,
        refresh_ttl: i64,
    ) -> Result<MobileTokenResponse, AppError> {
        let refresh_hash = AuthService::hash_token(refresh_token);

        // Find session by installation_id + verify hash
        let session = mobile_session::Entity::find()
            .filter(mobile_session::Column::InstallationId.eq(installation_id))
            .filter(mobile_session::Column::RevokedAt.is_null())
            .one(db)
            .await?
            .ok_or(AppError::Unauthorized)?;

        // Verify hash matches
        if session.refresh_token_hash != refresh_hash {
            // Possible token reuse attack — revoke the session
            Self::revoke_session(db, &session.id).await?;
            return Err(AppError::Unauthorized);
        }

        // Check expiry
        if session.expires_at < Utc::now() {
            Self::revoke_session(db, &session.id).await?;
            return Err(AppError::Unauthorized);
        }

        // Load user
        let user = user::Entity::find_by_id(&session.user_id)
            .one(db)
            .await?
            .ok_or(AppError::Unauthorized)?;

        // Generate new token pair
        let (access_token, access_expires_in_secs) = jwt
            .create_access_token(&user.id, &user.username, &user.role)
            .map_err(|e| AppError::Internal(format!("JWT error: {e}")))?;

        let new_refresh_token = Self::generate_refresh_token();
        let new_refresh_hash = AuthService::hash_token(&new_refresh_token);
        let now = Utc::now();

        // Update session with new refresh token (rotation)
        let mut update: mobile_session::ActiveModel = session.into();
        update.refresh_token_hash = Set(new_refresh_hash);
        update.expires_at = Set(now + chrono::Duration::seconds(refresh_ttl));
        update.updated_at = Set(now);
        update.update(db).await?;

        Ok(MobileTokenResponse {
            access_token,
            access_expires_in_secs,
            refresh_token: new_refresh_token,
            refresh_expires_in_secs: refresh_ttl,
            token_type: "Bearer".to_string(),
            user: MobileUser {
                id: user.id.clone(),
                username: user.username.clone(),
                role: user.role.clone(),
            },
        })
    }

    /// Logout: revokes session and clears device push token.
    pub async fn logout(
        db: &DatabaseConnection,
        _refresh_token: &str,
        installation_id: &str,
    ) -> Result<(), AppError> {
        Self::revoke_by_installation(db, installation_id).await?;
        Self::clear_device_push_token(db, installation_id).await?;
        Ok(())
    }

    fn generate_refresh_token() -> String {
        use base64::Engine;
        use rand::RngCore;
        let mut buf = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut buf);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
    }

    async fn revoke_by_installation(
        db: &DatabaseConnection,
        installation_id: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now();
        mobile_session::Entity::update_many()
            .filter(mobile_session::Column::InstallationId.eq(installation_id))
            .filter(mobile_session::Column::RevokedAt.is_null())
            .col_expr(
                mobile_session::Column::RevokedAt,
                Expr::value(Some(now)),
            )
            .col_expr(mobile_session::Column::UpdatedAt, Expr::value(now))
            .exec(db)
            .await?;
        Ok(())
    }

    async fn revoke_session(db: &DatabaseConnection, session_id: &str) -> Result<(), AppError> {
        let now = Utc::now();
        mobile_session::Entity::update_many()
            .filter(mobile_session::Column::Id.eq(session_id))
            .col_expr(
                mobile_session::Column::RevokedAt,
                Expr::value(Some(now)),
            )
            .col_expr(mobile_session::Column::UpdatedAt, Expr::value(now))
            .exec(db)
            .await?;
        Ok(())
    }

    async fn clear_device_push_token(
        db: &DatabaseConnection,
        installation_id: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now();
        mobile_device_registration::Entity::update_many()
            .filter(mobile_device_registration::Column::InstallationId.eq(installation_id))
            .col_expr(
                mobile_device_registration::Column::PushToken,
                Expr::value(Option::<String>::None),
            )
            .col_expr(
                mobile_device_registration::Column::DisabledAt,
                Expr::value(Some(now)),
            )
            .col_expr(
                mobile_device_registration::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(db)
            .await?;
        Ok(())
    }
}
