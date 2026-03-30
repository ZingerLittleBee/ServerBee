use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use rand::RngCore;
use sea_orm::*;
use serde::Serialize;
use uuid::Uuid;

use crate::config::MobileConfig;
use crate::entity::{device_token, mobile_session, session, user};
use crate::error::AppError;
use crate::service::auth::AuthService;

/// Parameters for mobile login.
pub struct MobileLoginParams<'a> {
    pub username: &'a str,
    pub password: &'a str,
    pub totp_code: Option<&'a str>,
    pub installation_id: &'a str,
    pub device_name: &'a str,
    pub ip: &'a str,
    pub user_agent: &'a str,
}

/// Token pair returned after successful login or refresh.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MobileTokenResponse {
    pub access_token: String,
    pub access_expires_in_secs: i64,
    pub refresh_token: String,
    pub refresh_expires_in_secs: i64,
    pub token_type: String,
    pub user: MobileUserResponse,
}

/// Minimal user info returned alongside tokens.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MobileUserResponse {
    pub id: String,
    pub username: String,
    pub role: String,
}

/// Summary of an active mobile device session.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MobileDeviceInfo {
    pub id: String,
    pub device_name: String,
    pub installation_id: String,
    pub created_at: chrono::DateTime<Utc>,
    pub last_used_at: chrono::DateTime<Utc>,
}

pub struct MobileAuthService;

impl MobileAuthService {
    // ── Token generation ─────────────────────────────────────────────────

    /// Generate a cryptographically random refresh token (32 bytes, base64url-no-pad).
    pub fn generate_refresh_token() -> String {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Hash a refresh token using argon2 with a random salt.
    pub fn hash_refresh_token(token: &str) -> Result<String, AppError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let hash = argon2
            .hash_password(token.as_bytes(), &salt)
            .map_err(|e| AppError::Internal(format!("Refresh token hashing failed: {e}")))?;
        Ok(hash.to_string())
    }

    /// Verify a refresh token against its argon2 hash.
    pub fn verify_refresh_token(token: &str, hash: &str) -> Result<bool, AppError> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| AppError::Internal(format!("Invalid refresh token hash: {e}")))?;
        Ok(Argon2::default()
            .verify_password(token.as_bytes(), &parsed_hash)
            .is_ok())
    }

    // ── Login ────────────────────────────────────────────────────────────

    /// Authenticate via username/password (+ optional TOTP), create a mobile session,
    /// and return a token pair.
    pub async fn login(
        db: &DatabaseConnection,
        config: &MobileConfig,
        params: MobileLoginParams<'_>,
    ) -> Result<MobileTokenResponse, AppError> {
        // Validate credentials using AuthService
        let user_model = Self::validate_credentials(
            db,
            params.username,
            params.password,
            params.totp_code,
        )
        .await?;

        Self::login_for_user(
            db,
            config,
            &user_model,
            params.installation_id,
            params.device_name,
            params.ip,
            params.user_agent,
        )
        .await
    }

    /// Issue tokens for an already-authenticated user (e.g. QR pairing).
    pub async fn login_for_user(
        db: &DatabaseConnection,
        config: &MobileConfig,
        user_model: &user::Model,
        installation_id: &str,
        device_name: &str,
        ip: &str,
        user_agent: &str,
    ) -> Result<MobileTokenResponse, AppError> {
        let now = Utc::now();

        // Generate token pair
        let access_token = AuthService::generate_session_token();
        let refresh_token = Self::generate_refresh_token();
        let refresh_token_hash = Self::hash_refresh_token(&refresh_token)?;

        let mobile_session_id = Uuid::new_v4().to_string();
        let refresh_expires_at = now + chrono::Duration::seconds(config.refresh_ttl);
        let access_expires_at = now + chrono::Duration::seconds(config.access_ttl);

        // Create mobile_session row
        let new_mobile_session = mobile_session::ActiveModel {
            id: Set(mobile_session_id.clone()),
            user_id: Set(user_model.id.clone()),
            refresh_token_hash: Set(refresh_token_hash),
            installation_id: Set(installation_id.to_string()),
            device_name: Set(device_name.to_string()),
            created_at: Set(now),
            expires_at: Set(refresh_expires_at),
            last_used_at: Set(now),
        };
        new_mobile_session.insert(db).await?;

        // Create session row (source = "mobile", linked to mobile_session)
        let new_session = session::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            user_id: Set(user_model.id.clone()),
            token: Set(access_token.clone()),
            ip: Set(ip.to_string()),
            user_agent: Set(user_agent.to_string()),
            expires_at: Set(access_expires_at),
            created_at: Set(now),
            source: Set("mobile".to_string()),
            mobile_session_id: Set(Some(mobile_session_id)),
        };
        new_session.insert(db).await?;

        Ok(MobileTokenResponse {
            access_token,
            access_expires_in_secs: config.access_ttl,
            refresh_token,
            refresh_expires_in_secs: config.refresh_ttl,
            token_type: "Bearer".to_string(),
            user: MobileUserResponse {
                id: user_model.id.clone(),
                username: user_model.username.clone(),
                role: user_model.role.clone(),
            },
        })
    }

    // ── Refresh ──────────────────────────────────────────────────────────

    /// Validate a refresh token, rotate to a new token pair.
    /// The old mobile session and its sessions are deleted, and a fresh pair is created.
    pub async fn refresh(
        db: &DatabaseConnection,
        config: &MobileConfig,
        refresh_token: &str,
        installation_id: &str,
        ip: &str,
        user_agent: &str,
    ) -> Result<MobileTokenResponse, AppError> {
        // Find all non-expired mobile sessions with the given installation_id
        let candidates = mobile_session::Entity::find()
            .filter(mobile_session::Column::InstallationId.eq(installation_id))
            .filter(mobile_session::Column::ExpiresAt.gt(Utc::now()))
            .all(db)
            .await?;

        // Find the matching session by verifying the refresh token hash
        let mut matched_session: Option<mobile_session::Model> = None;
        for candidate in candidates {
            if Self::verify_refresh_token(refresh_token, &candidate.refresh_token_hash)? {
                matched_session = Some(candidate);
                break;
            }
        }

        let old_session = matched_session.ok_or(AppError::Unauthorized)?;
        let user_id = old_session.user_id.clone();
        let device_name = old_session.device_name.clone();
        let old_session_id = old_session.id.clone();

        // Fetch user
        let user_model = user::Entity::find_by_id(&user_id)
            .one(db)
            .await?
            .ok_or(AppError::Unauthorized)?;

        // Delete old sessions linked to this mobile_session
        session::Entity::delete_many()
            .filter(session::Column::MobileSessionId.eq(&old_session_id))
            .exec(db)
            .await?;

        // Delete the old mobile_session
        mobile_session::Entity::delete_by_id(&old_session_id)
            .exec(db)
            .await?;

        // Issue a fresh token pair
        Self::login_for_user(
            db,
            config,
            &user_model,
            installation_id,
            &device_name,
            ip,
            user_agent,
        )
        .await
    }

    // ── Logout ───────────────────────────────────────────────────────────

    /// Delete a mobile session and all associated sessions and device tokens.
    /// This is called when the mobile client explicitly logs out.
    pub async fn logout(
        db: &DatabaseConnection,
        mobile_session_id: &str,
    ) -> Result<(), AppError> {
        Self::delete_mobile_session_cascade(db, mobile_session_id).await
    }

    // ── Device listing / revocation ──────────────────────────────────────

    /// List active (non-expired) mobile sessions for a user.
    pub async fn list_devices(
        db: &DatabaseConnection,
        user_id: &str,
    ) -> Result<Vec<MobileDeviceInfo>, AppError> {
        let sessions = mobile_session::Entity::find()
            .filter(mobile_session::Column::UserId.eq(user_id))
            .filter(mobile_session::Column::ExpiresAt.gt(Utc::now()))
            .order_by_desc(mobile_session::Column::LastUsedAt)
            .all(db)
            .await?;

        let devices = sessions
            .into_iter()
            .map(|s| MobileDeviceInfo {
                id: s.id,
                device_name: s.device_name,
                installation_id: s.installation_id,
                created_at: s.created_at,
                last_used_at: s.last_used_at,
            })
            .collect();

        Ok(devices)
    }

    /// Revoke a mobile session by ID. The caller must own the session (ownership check).
    pub async fn revoke_device(
        db: &DatabaseConnection,
        mobile_session_id: &str,
        user_id: &str,
    ) -> Result<(), AppError> {
        // Verify ownership
        let ms = mobile_session::Entity::find_by_id(mobile_session_id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound(
                "Mobile session not found".to_string(),
            ))?;

        if ms.user_id != user_id {
            return Err(AppError::Forbidden(
                "Cannot revoke another user's device".to_string(),
            ));
        }

        Self::delete_mobile_session_cascade(db, mobile_session_id).await
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    /// Validate user credentials (username + password + optional TOTP).
    async fn validate_credentials(
        db: &DatabaseConnection,
        username: &str,
        password: &str,
        totp_code: Option<&str>,
    ) -> Result<user::Model, AppError> {
        let user_model = user::Entity::find()
            .filter(user::Column::Username.eq(username))
            .one(db)
            .await?
            .ok_or(AppError::Unauthorized)?;

        let valid = AuthService::verify_password(password, &user_model.password_hash)?;
        if !valid {
            return Err(AppError::Unauthorized);
        }

        // Check 2FA
        if let Some(ref secret) = user_model.totp_secret {
            match totp_code {
                Some(code) => {
                    if !AuthService::verify_totp(secret, code)? {
                        return Err(AppError::Unauthorized);
                    }
                }
                None => {
                    return Err(AppError::Validation("2fa_required".to_string()));
                }
            }
        }

        Ok(user_model)
    }

    /// Delete a mobile session and cascade-delete its linked sessions and device tokens.
    async fn delete_mobile_session_cascade(
        db: &DatabaseConnection,
        mobile_session_id: &str,
    ) -> Result<(), AppError> {
        // Delete associated sessions
        session::Entity::delete_many()
            .filter(session::Column::MobileSessionId.eq(mobile_session_id))
            .exec(db)
            .await?;

        // Delete associated device tokens
        device_token::Entity::delete_many()
            .filter(device_token::Column::MobileSessionId.eq(mobile_session_id))
            .exec(db)
            .await?;

        // Delete the mobile session itself
        mobile_session::Entity::delete_by_id(mobile_session_id)
            .exec(db)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refresh_token_hash_roundtrip() {
        let token = MobileAuthService::generate_refresh_token();
        let hash = MobileAuthService::hash_refresh_token(&token).unwrap();
        assert!(MobileAuthService::verify_refresh_token(&token, &hash).unwrap());
        assert!(!MobileAuthService::verify_refresh_token("wrong_token", &hash).unwrap());
    }

    #[test]
    fn test_generate_refresh_token_format() {
        let token = MobileAuthService::generate_refresh_token();
        assert!(!token.is_empty(), "refresh token must not be empty");
        // 32 bytes base64url-encoded (no padding) => 43 characters
        assert_eq!(
            token.len(),
            43,
            "32-byte base64url-no-pad token should be 43 chars, got {}",
            token.len()
        );
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "token must only contain base64url characters"
        );
    }

    #[test]
    fn test_generate_refresh_token_uniqueness() {
        let t1 = MobileAuthService::generate_refresh_token();
        let t2 = MobileAuthService::generate_refresh_token();
        assert_ne!(t1, t2, "two generated tokens must be different");
    }

    #[test]
    fn test_hash_refresh_token_produces_argon2() {
        let token = MobileAuthService::generate_refresh_token();
        let hash = MobileAuthService::hash_refresh_token(&token).unwrap();
        assert!(
            hash.starts_with("$argon2"),
            "hash should be in argon2 PHC format, got: {hash}"
        );
    }

    #[test]
    fn test_verify_refresh_token_invalid_hash() {
        let result = MobileAuthService::verify_refresh_token("some_token", "not_a_valid_hash");
        assert!(
            result.is_err(),
            "verifying against an invalid hash format should return an error"
        );
    }

    // ── DB integration tests ──────────────────────────────────────────────────

    use crate::config::MobileConfig;
    use crate::service::auth::AuthService;
    use crate::test_utils::setup_test_db;

    fn default_mobile_config() -> MobileConfig {
        MobileConfig {
            access_ttl: 900,
            refresh_ttl: 2_592_000,
        }
    }

    #[tokio::test]
    async fn test_login_success() {
        let (db, _tmp) = setup_test_db().await;
        let config = default_mobile_config();

        AuthService::create_user(&db, "alice", "pass123", "admin")
            .await
            .expect("create_user should succeed");

        let result = MobileAuthService::login(
            &db,
            &config,
            MobileLoginParams {
                username: "alice",
                password: "pass123",
                totp_code: None,
                installation_id: "inst-001",
                device_name: "Alice's iPhone",
                ip: "127.0.0.1",
                user_agent: "ServerBee-iOS/1.0",
            },
        )
        .await
        .expect("login should succeed");

        assert!(!result.access_token.is_empty());
        assert!(!result.refresh_token.is_empty());
        assert_eq!(result.token_type, "Bearer");
        assert_eq!(result.user.username, "alice");
        assert_eq!(result.user.role, "admin");
        assert_eq!(result.access_expires_in_secs, 900);
        assert_eq!(result.refresh_expires_in_secs, 2_592_000);
    }

    #[tokio::test]
    async fn test_login_wrong_password() {
        let (db, _tmp) = setup_test_db().await;
        let config = default_mobile_config();

        AuthService::create_user(&db, "bob", "correct_pass", "member")
            .await
            .expect("create_user should succeed");

        let result = MobileAuthService::login(
            &db,
            &config,
            MobileLoginParams {
                username: "bob",
                password: "wrong_pass",
                totp_code: None,
                installation_id: "inst-002",
                device_name: "Bob's Android",
                ip: "127.0.0.1",
                user_agent: "ServerBee-Android/1.0",
            },
        )
        .await;

        assert!(result.is_err(), "wrong password should return an error");
    }

    #[tokio::test]
    async fn test_refresh_success() {
        let (db, _tmp) = setup_test_db().await;
        let config = default_mobile_config();

        AuthService::create_user(&db, "carol", "pass456", "member")
            .await
            .expect("create_user should succeed");

        let first = MobileAuthService::login(
            &db,
            &config,
            MobileLoginParams {
                username: "carol",
                password: "pass456",
                totp_code: None,
                installation_id: "inst-003",
                device_name: "Carol's Phone",
                ip: "127.0.0.1",
                user_agent: "ServerBee-iOS/1.0",
            },
        )
        .await
        .expect("login should succeed");

        let refreshed = MobileAuthService::refresh(
            &db,
            &config,
            &first.refresh_token,
            "inst-003",
            "127.0.0.1",
            "ServerBee-iOS/1.0",
        )
        .await
        .expect("refresh should succeed");

        // New tokens should differ from old ones
        assert_ne!(refreshed.access_token, first.access_token);
        assert_ne!(refreshed.refresh_token, first.refresh_token);
        assert_eq!(refreshed.user.username, "carol");
    }

    #[tokio::test]
    async fn test_refresh_wrong_installation_id() {
        let (db, _tmp) = setup_test_db().await;
        let config = default_mobile_config();

        AuthService::create_user(&db, "dave", "pass789", "member")
            .await
            .expect("create_user should succeed");

        let first = MobileAuthService::login(
            &db,
            &config,
            MobileLoginParams {
                username: "dave",
                password: "pass789",
                totp_code: None,
                installation_id: "inst-004",
                device_name: "Dave's Phone",
                ip: "127.0.0.1",
                user_agent: "ServerBee-Android/1.0",
            },
        )
        .await
        .expect("login should succeed");

        let result = MobileAuthService::refresh(
            &db,
            &config,
            &first.refresh_token,
            "wrong-installation-id",
            "127.0.0.1",
            "ServerBee-Android/1.0",
        )
        .await;

        assert!(
            result.is_err(),
            "refresh with wrong installation_id should fail"
        );
    }

    #[tokio::test]
    async fn test_list_devices() {
        let (db, _tmp) = setup_test_db().await;
        let config = default_mobile_config();

        let user = AuthService::create_user(&db, "eve", "passABC", "admin")
            .await
            .expect("create_user should succeed");

        // Login from two devices
        MobileAuthService::login(
            &db,
            &config,
            MobileLoginParams {
                username: "eve",
                password: "passABC",
                totp_code: None,
                installation_id: "inst-A",
                device_name: "iPhone",
                ip: "127.0.0.1",
                user_agent: "ServerBee-iOS/1.0",
            },
        )
        .await
        .expect("first login should succeed");

        MobileAuthService::login(
            &db,
            &config,
            MobileLoginParams {
                username: "eve",
                password: "passABC",
                totp_code: None,
                installation_id: "inst-B",
                device_name: "Android",
                ip: "127.0.0.1",
                user_agent: "ServerBee-Android/1.0",
            },
        )
        .await
        .expect("second login should succeed");

        let devices = MobileAuthService::list_devices(&db, &user.id)
            .await
            .expect("list_devices should succeed");

        assert_eq!(devices.len(), 2);
    }

    #[tokio::test]
    async fn test_revoke_device() {
        let (db, _tmp) = setup_test_db().await;
        let config = default_mobile_config();

        let user = AuthService::create_user(&db, "frank", "passXYZ", "admin")
            .await
            .expect("create_user should succeed");

        MobileAuthService::login(
            &db,
            &config,
            MobileLoginParams {
                username: "frank",
                password: "passXYZ",
                totp_code: None,
                installation_id: "inst-F",
                device_name: "Frank's Phone",
                ip: "127.0.0.1",
                user_agent: "ServerBee-iOS/1.0",
            },
        )
        .await
        .expect("login should succeed");

        let devices = MobileAuthService::list_devices(&db, &user.id)
            .await
            .expect("list_devices should succeed");
        assert_eq!(devices.len(), 1);

        MobileAuthService::revoke_device(&db, &devices[0].id, &user.id)
            .await
            .expect("revoke_device should succeed");

        let devices_after = MobileAuthService::list_devices(&db, &user.id)
            .await
            .expect("list_devices should succeed");
        assert_eq!(devices_after.len(), 0);
    }

    #[tokio::test]
    async fn test_revoke_device_wrong_user() {
        let (db, _tmp) = setup_test_db().await;
        let config = default_mobile_config();

        let user_a = AuthService::create_user(&db, "userA", "passA", "admin")
            .await
            .expect("create userA should succeed");
        let user_b = AuthService::create_user(&db, "userB", "passB", "member")
            .await
            .expect("create userB should succeed");

        MobileAuthService::login(
            &db,
            &config,
            MobileLoginParams {
                username: "userA",
                password: "passA",
                totp_code: None,
                installation_id: "inst-A",
                device_name: "Phone A",
                ip: "127.0.0.1",
                user_agent: "ServerBee/1.0",
            },
        )
        .await
        .expect("login should succeed");

        let devices = MobileAuthService::list_devices(&db, &user_a.id)
            .await
            .expect("list_devices should succeed");
        assert_eq!(devices.len(), 1);

        // userB tries to revoke userA's device
        let result =
            MobileAuthService::revoke_device(&db, &devices[0].id, &user_b.id).await;
        assert!(result.is_err(), "revoking another user's device should fail");
    }

    #[tokio::test]
    async fn test_logout_cleans_up() {
        let (db, _tmp) = setup_test_db().await;
        let config = default_mobile_config();

        let user = AuthService::create_user(&db, "grace", "passG", "member")
            .await
            .expect("create_user should succeed");

        MobileAuthService::login(
            &db,
            &config,
            MobileLoginParams {
                username: "grace",
                password: "passG",
                totp_code: None,
                installation_id: "inst-G",
                device_name: "Grace's Phone",
                ip: "127.0.0.1",
                user_agent: "ServerBee/1.0",
            },
        )
        .await
        .expect("login should succeed");

        let devices = MobileAuthService::list_devices(&db, &user.id)
            .await
            .expect("list_devices should succeed");
        assert_eq!(devices.len(), 1);

        MobileAuthService::logout(&db, &devices[0].id)
            .await
            .expect("logout should succeed");

        let devices_after = MobileAuthService::list_devices(&db, &user.id)
            .await
            .expect("list_devices should succeed");
        assert_eq!(devices_after.len(), 0);

        // Associated session rows should also be gone
        let sessions = session::Entity::find()
            .filter(session::Column::UserId.eq(&user.id))
            .filter(session::Column::Source.eq("mobile"))
            .all(&db)
            .await
            .expect("query sessions");
        assert_eq!(sessions.len(), 0, "mobile sessions should be cleaned up");
    }
}
