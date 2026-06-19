use argon2::password_hash::SaltString;
use argon2::password_hash::rand_core::OsRng;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use rand::RngCore;
use sea_orm::*;
use uuid::Uuid;

use sea_orm::sea_query::Expr;

use crate::entity::{api_key, device_token, mobile_session, server, session, user};
use crate::error::AppError;

/// Parameters for creating an authenticated session.
pub struct LoginParams<'a> {
    pub username: &'a str,
    pub password: &'a str,
    pub totp_code: Option<&'a str>,
    pub ip: &'a str,
    pub user_agent: &'a str,
    pub session_ttl: i64,
}

pub struct AuthService;

impl AuthService {
    /// The fixed username for the auto-provisioned first admin account.
    pub const DEFAULT_ADMIN_USERNAME: &str = "admin";

    /// Minimum length for a user-chosen password.
    pub const MIN_PASSWORD_LEN: usize = 8;

    /// Validate a user-chosen password against the minimum strength policy.
    /// Applied wherever a user sets their own password (onboarding, change
    /// password); not applied to system-generated secrets.
    pub fn validate_password_strength(password: &str) -> Result<(), AppError> {
        if password.chars().count() < Self::MIN_PASSWORD_LEN {
            return Err(AppError::Validation(format!(
                "Password must be at least {} characters",
                Self::MIN_PASSWORD_LEN
            )));
        }
        Ok(())
    }

    /// Hash a password using argon2 with a random salt.
    pub fn hash_password(password: &str) -> Result<String, AppError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| AppError::Internal(format!("Password hashing failed: {e}")))?;
        Ok(hash.to_string())
    }

    /// Verify a password against an argon2 hash.
    pub fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| AppError::Internal(format!("Invalid password hash: {e}")))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }

    /// Create a new user with the given username, password, and role.
    pub async fn create_user(
        db: &DatabaseConnection,
        username: &str,
        password: &str,
        role: &str,
    ) -> Result<user::Model, AppError> {
        // Check if username already exists
        let existing = user::Entity::find()
            .filter(user::Column::Username.eq(username))
            .one(db)
            .await?;

        if existing.is_some() {
            return Err(AppError::Conflict(format!(
                "User '{username}' already exists"
            )));
        }

        let password_hash = Self::hash_password(password)?;
        let now = Utc::now();

        let new_user = user::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            username: Set(username.to_string()),
            password_hash: Set(password_hash),
            role: Set(role.to_string()),
            totp_secret: Set(None),
            must_change_password: Set(false),
            password_changed_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        };

        let result = new_user.insert(db).await?;
        Ok(result)
    }

    /// Authenticate a user by username and password, creating a new session.
    /// If the user has 2FA enabled, `totp_code` must be provided.
    /// Returns the session and user models on success.
    pub async fn login(
        db: &DatabaseConnection,
        params: LoginParams<'_>,
    ) -> Result<(session::Model, user::Model), AppError> {
        let user = user::Entity::find()
            .filter(user::Column::Username.eq(params.username))
            .one(db)
            .await?
            .ok_or(AppError::Unauthorized)?;

        let valid = Self::verify_password(params.password, &user.password_hash)?;
        if !valid {
            return Err(AppError::Unauthorized);
        }

        // Check 2FA
        if let Some(ref secret) = user.totp_secret {
            match params.totp_code {
                Some(code) => {
                    if !Self::verify_totp(secret, code)? {
                        return Err(AppError::Unauthorized);
                    }
                }
                None => {
                    // 2FA enabled but no code provided — signal requires_2fa
                    return Err(AppError::Validation("2fa_required".to_string()));
                }
            }
        }

        let token = Self::generate_session_token();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(params.session_ttl);

        let new_session = session::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            user_id: Set(user.id.clone()),
            token: Set(token),
            ip: Set(params.ip.to_string()),
            user_agent: Set(params.user_agent.to_string()),
            expires_at: Set(expires_at),
            created_at: Set(now),
            source: Set("web".to_string()),
            mobile_session_id: Set(None),
        };

        let session_model = new_session.insert(db).await?;
        Ok((session_model, user))
    }

    /// Validate a session token. If valid and not expired, returns the
    /// associated user and session. Only performs sliding expiry for
    /// `source == "web"` sessions; mobile sessions keep their original TTL.
    pub async fn validate_session(
        db: &DatabaseConnection,
        token: &str,
        web_session_ttl: i64,
    ) -> Result<Option<(user::Model, session::Model)>, AppError> {
        let session = session::Entity::find()
            .filter(session::Column::Token.eq(token))
            .one(db)
            .await?;

        let session = match session {
            Some(s) => s,
            None => return Ok(None),
        };

        // Check expiration
        if session.expires_at < Utc::now() {
            // Clean up expired session
            session::Entity::delete_by_id(&session.id).exec(db).await?;
            return Ok(None);
        }

        // Sliding expiry: only extend for web sessions
        let session = if session.source == "web" {
            let new_expires = Utc::now() + chrono::Duration::seconds(web_session_ttl);
            let mut active: session::ActiveModel = session.into();
            active.expires_at = Set(new_expires);
            active.update(db).await?
        } else {
            // Update mobile_session.last_used_at (fire-and-forget for latency)
            if let Some(ref ms_id) = session.mobile_session_id {
                let ms_id = ms_id.clone();
                let db = db.clone();
                tokio::spawn(async move {
                    let _ = mobile_session::Entity::update_many()
                        .col_expr(mobile_session::Column::LastUsedAt, Expr::value(Utc::now()))
                        .filter(mobile_session::Column::Id.eq(&ms_id))
                        .exec(&db)
                        .await;
                });
            }
            session
        };

        // Fetch the user
        let user = user::Entity::find_by_id(&session.user_id).one(db).await?;

        match user {
            Some(u) => Ok(Some((u, session))),
            None => Ok(None),
        }
    }

    /// Delete a session by its token (logout).
    pub async fn logout(db: &DatabaseConnection, token: &str) -> Result<(), AppError> {
        session::Entity::delete_many()
            .filter(session::Column::Token.eq(token))
            .exec(db)
            .await?;
        Ok(())
    }

    /// Create a new API key for a user. Returns the model and the plaintext key.
    /// The key has the format "serverbee_" + random base64url bytes.
    /// Only the argon2 hash and a prefix (first 8 chars after "serverbee_") are stored.
    pub async fn create_api_key(
        db: &DatabaseConnection,
        user_id: &str,
        name: &str,
    ) -> Result<(api_key::Model, String), AppError> {
        let raw_key = Self::generate_api_key_raw();
        let after_prefix = &raw_key[10..]; // strip "serverbee_"
        let key_prefix = &after_prefix[..8.min(after_prefix.len())];
        let key_hash = Self::hash_password(&raw_key)?;
        let now = Utc::now();

        let new_key = api_key::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            user_id: Set(user_id.to_string()),
            name: Set(name.to_string()),
            key_hash: Set(key_hash),
            key_prefix: Set(key_prefix.to_string()),
            last_used_at: Set(None),
            created_at: Set(now),
        };

        let model = new_key.insert(db).await?;
        Ok((model, raw_key))
    }

    /// List all API keys for a user.
    pub async fn list_api_keys(
        db: &DatabaseConnection,
        user_id: &str,
    ) -> Result<Vec<api_key::Model>, AppError> {
        let keys = api_key::Entity::find()
            .filter(api_key::Column::UserId.eq(user_id))
            .all(db)
            .await?;
        Ok(keys)
    }

    /// Delete an API key by ID, ensuring it belongs to the given user.
    pub async fn delete_api_key(
        db: &DatabaseConnection,
        id: &str,
        user_id: &str,
    ) -> Result<(), AppError> {
        let result = api_key::Entity::delete_many()
            .filter(api_key::Column::Id.eq(id))
            .filter(api_key::Column::UserId.eq(user_id))
            .exec(db)
            .await?;

        if result.rows_affected == 0 {
            return Err(AppError::NotFound("API key not found".to_string()));
        }

        Ok(())
    }

    /// Validate an API key. Extracts the prefix, searches by key_prefix,
    /// then verifies with argon2. Updates last_used_at on success.
    pub async fn validate_api_key(
        db: &DatabaseConnection,
        key: &str,
    ) -> Result<Option<user::Model>, AppError> {
        if !key.starts_with("serverbee_") || key.len() < 18 {
            return Ok(None);
        }

        let after_prefix = &key[10..];
        let key_prefix = &after_prefix[..8.min(after_prefix.len())];

        let candidates = api_key::Entity::find()
            .filter(api_key::Column::KeyPrefix.eq(key_prefix))
            .all(db)
            .await?;

        for candidate in candidates {
            if Self::verify_password(key, &candidate.key_hash)? {
                // Update last_used_at
                let candidate_user_id = candidate.user_id.clone();
                let mut active: api_key::ActiveModel = candidate.into();
                active.last_used_at = Set(Some(Utc::now()));
                active.update(db).await?;

                // Fetch user
                let user = user::Entity::find_by_id(&candidate_user_id).one(db).await?;
                return Ok(user);
            }
        }

        Ok(None)
    }

    /// Validate an agent token by searching servers by token_prefix,
    /// then verifying with argon2.
    pub async fn validate_agent_token(
        db: &DatabaseConnection,
        token: &str,
    ) -> Result<Option<server::Model>, AppError> {
        if token.len() < 8 {
            return Ok(None);
        }

        let token_prefix = &token[..8.min(token.len())];

        let candidates = server::Entity::find()
            .filter(server::Column::TokenPrefix.eq(token_prefix))
            .filter(server::Column::TokenHash.is_not_null())
            .all(db)
            .await?;

        for candidate in candidates {
            // Defense-in-depth: the query already filters NULL token_hash rows,
            // but keep this guard in case a row slips through (e.g., schema drift).
            let Some(hash) = candidate.token_hash.as_deref() else {
                continue;
            };
            if Self::verify_password(token, hash)? {
                return Ok(Some(candidate));
            }
        }

        Ok(None)
    }

    /// Initialize the admin user if the users table is empty.
    /// Always generates a random password and forces a change on first login.
    /// Returns `Some(generated_password)` when a new admin was created.
    pub async fn init_admin(db: &DatabaseConnection) -> Result<Option<String>, AppError> {
        let user_count = user::Entity::find().count(db).await?;
        if user_count > 0 {
            return Ok(None);
        }

        let password = Self::generate_session_token();
        let created =
            Self::create_user(db, Self::DEFAULT_ADMIN_USERNAME, &password, "admin").await?;

        let mut active: user::ActiveModel = created.into();
        active.must_change_password = Set(true);
        active.updated_at = Set(Utc::now());
        active.update(db).await?;

        tracing::info!(
            "Admin user '{}' created with a random password (must be changed on first login)",
            Self::DEFAULT_ADMIN_USERNAME
        );

        Ok(Some(password))
    }

    /// Generate a cryptographically random session token (32 bytes, base64url encoded).
    pub fn generate_session_token() -> String {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Generate a raw API key: "serverbee_" + 32 random bytes (base64url encoded).
    pub fn generate_api_key_raw() -> String {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        format!("serverbee_{}", URL_SAFE_NO_PAD.encode(bytes))
    }

    /// Check if the given TOTP code is valid for the user's secret.
    pub fn verify_totp(secret: &str, code: &str) -> Result<bool, AppError> {
        use totp_rs::{Algorithm, Secret, TOTP};

        let secret_bytes = Secret::Encoded(secret.to_string())
            .to_bytes()
            .map_err(|e| AppError::Internal(format!("Invalid TOTP secret: {e}")))?;

        let totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            secret_bytes,
            Some("ServerBee".to_string()),
            String::new(),
        )
        .map_err(|e| AppError::Internal(format!("TOTP error: {e}")))?;

        Ok(totp.check_current(code).unwrap_or(false))
    }

    /// Generate a new TOTP secret and return (secret_base32, otpauth_url, qr_code_base64).
    pub fn generate_totp_secret(username: &str) -> Result<(String, String, String), AppError> {
        use totp_rs::{Algorithm, Secret, TOTP};

        let secret = Secret::generate_secret();
        let secret_base32 = secret.to_encoded().to_string();
        let secret_bytes = secret
            .to_bytes()
            .map_err(|e| AppError::Internal(format!("Secret error: {e}")))?;

        let totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            secret_bytes,
            Some("ServerBee".to_string()),
            username.to_string(),
        )
        .map_err(|e| AppError::Internal(format!("TOTP error: {e}")))?;

        let url = totp.get_url();
        let qr_base64 = totp
            .get_qr_base64()
            .map_err(|e| AppError::Internal(format!("QR error: {e}")))?;

        Ok((secret_base32, url, qr_base64))
    }

    /// Enable 2FA for a user by saving the TOTP secret.
    pub async fn enable_2fa(
        db: &DatabaseConnection,
        user_id: &str,
        secret: &str,
    ) -> Result<(), AppError> {
        let user = user::Entity::find_by_id(user_id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound("User not found".to_string()))?;

        let mut active: user::ActiveModel = user.into();
        active.totp_secret = Set(Some(secret.to_string()));
        active.updated_at = Set(Utc::now());
        active.update(db).await?;
        Ok(())
    }

    /// Disable 2FA for a user.
    pub async fn disable_2fa(db: &DatabaseConnection, user_id: &str) -> Result<(), AppError> {
        let user = user::Entity::find_by_id(user_id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound("User not found".to_string()))?;

        let mut active: user::ActiveModel = user.into();
        active.totp_secret = Set(None);
        active.updated_at = Set(Utc::now());
        active.update(db).await?;
        Ok(())
    }

    /// Check if a user has 2FA enabled.
    pub async fn has_2fa(db: &DatabaseConnection, user_id: &str) -> Result<bool, AppError> {
        let user = user::Entity::find_by_id(user_id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound("User not found".to_string()))?;
        Ok(user.totp_secret.is_some())
    }

    /// Change a user's password after verifying the old password.
    pub async fn change_password(
        db: &DatabaseConnection,
        user_id: &str,
        old_password: &str,
        new_password: &str,
        keep_session_token: Option<&str>,
    ) -> Result<(), AppError> {
        let user = user::Entity::find_by_id(user_id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound("User not found".to_string()))?;

        let valid = Self::verify_password(old_password, &user.password_hash)?;
        if !valid {
            return Err(AppError::BadRequest(
                "Current password is incorrect".to_string(),
            ));
        }

        Self::validate_password_strength(new_password)?;

        let new_hash = Self::hash_password(new_password)?;
        let now = Utc::now();

        // Resolve the caller's own mobile session to preserve (when the kept
        // session is itself a mobile one, so the active device isn't logged out
        // by its own password change). Read-only lookup, done before the txn.
        let keep_mobile_session_id = match keep_session_token {
            Some(token) => session::Entity::find()
                .filter(session::Column::Token.eq(token))
                .one(db)
                .await?
                .and_then(|s| s.mobile_session_id),
            None => None,
        };

        // Apply the password change and every revocation atomically, so a
        // failure can't leave the password rotated while sessions stay live
        // (and a concurrent push_register can't wedge a half-applied state).
        let txn = db.begin().await?;

        let mut active: user::ActiveModel = user.into();
        active.password_hash = Set(new_hash);
        // Stamp the change so mobile refresh rejects refresh tokens whose
        // session was issued earlier — the authoritative guard against a stolen
        // refresh token surviving the change (see MobileAuthService::refresh).
        active.password_changed_at = Set(Some(now));
        active.updated_at = Set(now);
        active.update(&txn).await?;

        // Revoke the user's other web sessions. Keep the caller's current one
        // when its token is known (web cookie / bearer flow), else revoke all.
        let mut revoke =
            session::Entity::delete_many().filter(session::Column::UserId.eq(user_id));
        if let Some(token) = keep_session_token {
            revoke = revoke.filter(session::Column::Token.ne(token));
        }
        revoke.exec(&txn).await?;

        // The mobile refresh secret lives in `mobile_session` (a separate
        // table), so the revocation above does NOT cover the mobile auth path.
        Self::revoke_user_mobile_sessions(&txn, user_id, keep_mobile_session_id.as_deref()).await?;

        // The caller's own mobile session is intentionally preserved (above).
        // Bump its issuance time to the change instant so token versioning keeps
        // accepting its refresh token — it predates the change, but it IS the
        // caller's current, deliberately-kept session (the caller proved the old
        // password and holds this session's token).
        if let Some(keep) = keep_mobile_session_id.as_deref()
            && let Some(ms) = mobile_session::Entity::find_by_id(keep).one(&txn).await?
        {
            let mut ms: mobile_session::ActiveModel = ms.into();
            ms.created_at = Set(now);
            ms.update(&txn).await?;
        }

        txn.commit().await?;
        Ok(())
    }

    /// Revoke a user's mobile sessions (refresh tokens live in `mobile_session`,
    /// which plain session revocation does not touch). Optionally preserve one
    /// mobile session by id (the caller's own). `device_token` references
    /// `mobile_session` with no ON DELETE cascade, so its rows are removed first.
    pub async fn revoke_user_mobile_sessions<C: ConnectionTrait>(
        conn: &C,
        user_id: &str,
        keep_mobile_session_id: Option<&str>,
    ) -> Result<(), AppError> {
        // Resolve the target set as a SUBQUERY by user_id, not a captured id
        // snapshot: a mobile_session created concurrently (e.g. by a
        // refresh-rotation race) is then still covered by these deletes rather
        // than slipping past a stale id list.
        let target_subquery = || {
            let mut q = mobile_session::Entity::find()
                .select_only()
                .column(mobile_session::Column::Id)
                .filter(mobile_session::Column::UserId.eq(user_id));
            if let Some(keep) = keep_mobile_session_id {
                q = q.filter(mobile_session::Column::Id.ne(keep));
            }
            q.into_query()
        };

        // device_token -> session -> mobile_session (FK order; device_token's
        // FK to mobile_session has no ON DELETE cascade). Sessions/tokens are
        // matched via the subquery; mobile_session is deleted directly by
        // user_id so any concurrently inserted row is caught too.
        device_token::Entity::delete_many()
            .filter(device_token::Column::MobileSessionId.in_subquery(target_subquery()))
            .exec(conn)
            .await?;
        session::Entity::delete_many()
            .filter(session::Column::MobileSessionId.in_subquery(target_subquery()))
            .exec(conn)
            .await?;
        let mut del =
            mobile_session::Entity::delete_many().filter(mobile_session::Column::UserId.eq(user_id));
        if let Some(keep) = keep_mobile_session_id {
            del = del.filter(mobile_session::Column::Id.ne(keep));
        }
        del.exec(conn).await?;

        Ok(())
    }

    /// Complete first-login onboarding: set a new password and optionally a new
    /// username, then clear the `must_change_password` flag. Only valid while
    /// the flag is set. Does NOT write audit logs (the handler does).
    pub async fn complete_onboarding(
        db: &DatabaseConnection,
        user_id: &str,
        new_password: &str,
        new_username: Option<&str>,
    ) -> Result<(), AppError> {
        let user = user::Entity::find_by_id(user_id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound("User not found".to_string()))?;

        if !user.must_change_password {
            return Err(AppError::Forbidden(
                "Onboarding is not required for this account".to_string(),
            ));
        }
        if new_password.is_empty() {
            return Err(AppError::Validation("New password is required".to_string()));
        }
        Self::validate_password_strength(new_password)?;
        if Self::verify_password(new_password, &user.password_hash)? {
            return Err(AppError::Validation(
                "New password must be different from the current password".to_string(),
            ));
        }

        let trimmed_username: Option<String> = new_username
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .filter(|u| *u != user.username)
            .map(str::to_string);

        if let Some(ref uname) = trimmed_username {
            let existing = user::Entity::find()
                .filter(user::Column::Username.eq(uname.as_str()))
                .one(db)
                .await?;
            if existing.is_some() {
                return Err(AppError::Conflict(format!(
                    "User '{uname}' already exists"
                )));
            }
        }

        let new_hash = Self::hash_password(new_password)?;
        let mut active: user::ActiveModel = user.into();
        active.password_hash = Set(new_hash);
        if let Some(uname) = trimmed_username {
            active.username = Set(uname);
        }
        active.must_change_password = Set(false);
        active.updated_at = Set(Utc::now());
        active.update(db).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build LoginParams with test defaults; override only what matters.
    fn login_params<'a>(username: &'a str, password: &'a str) -> LoginParams<'a> {
        LoginParams {
            username,
            password,
            totp_code: None,
            ip: "127.0.0.1",
            user_agent: "test-agent",
            session_ttl: 3600,
        }
    }

    #[test]
    fn test_hash_and_verify_password() {
        let password = "my_secret_p@ssw0rd!";
        let hash = AuthService::hash_password(password).expect("hashing should succeed");

        // Correct password should verify successfully
        let valid = AuthService::verify_password(password, &hash).expect("verify should succeed");
        assert!(valid, "correct password must verify as true");

        // Wrong password should fail verification
        let invalid =
            AuthService::verify_password("wrong_password", &hash).expect("verify should succeed");
        assert!(!invalid, "wrong password must verify as false");
    }

    #[test]
    fn test_hash_password_not_empty() {
        let hash = AuthService::hash_password("test123").expect("hashing should succeed");
        assert!(!hash.is_empty(), "hash output must not be empty");
        // Argon2 hashes start with "$argon2"
        assert!(
            hash.starts_with("$argon2"),
            "hash should be in argon2 PHC format, got: {hash}"
        );
    }

    #[test]
    fn test_hash_password_unique_salts() {
        let password = "same_password";
        let hash1 = AuthService::hash_password(password).expect("hash 1");
        let hash2 = AuthService::hash_password(password).expect("hash 2");

        // Two hashes of the same password should differ (random salt)
        assert_ne!(
            hash1, hash2,
            "hashing the same password twice must produce different hashes"
        );
    }

    #[test]
    fn test_generate_session_token() {
        let token = AuthService::generate_session_token();

        assert!(!token.is_empty(), "session token must not be empty");

        // 32 bytes base64url-encoded (no padding) => 43 characters
        assert_eq!(
            token.len(),
            43,
            "32-byte base64url-no-pad token should be 43 chars, got {}",
            token.len()
        );

        // Must be valid base64url characters (A-Z, a-z, 0-9, -, _)
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "token must only contain base64url characters"
        );
    }

    #[test]
    fn test_generate_session_token_uniqueness() {
        let t1 = AuthService::generate_session_token();
        let t2 = AuthService::generate_session_token();

        assert_ne!(t1, t2, "two generated tokens must be different");
    }

    #[test]
    fn test_generate_api_key_raw() {
        let key = AuthService::generate_api_key_raw();

        assert!(
            key.starts_with("serverbee_"),
            "API key must start with 'serverbee_' prefix"
        );
        // "serverbee_" + 43 chars of base64url = 53 total
        assert_eq!(
            key.len(),
            53,
            "API key should be 53 chars (serverbee_ + 43), got {}",
            key.len()
        );
    }

    #[test]
    fn test_verify_password_invalid_hash_format() {
        let result = AuthService::verify_password("password", "not_a_valid_hash");
        assert!(
            result.is_err(),
            "verifying against an invalid hash format should return an error"
        );
    }

    #[test]
    fn test_generate_totp_secret() {
        let (secret, url, qr) =
            AuthService::generate_totp_secret("testuser").expect("TOTP generation should succeed");

        assert!(!secret.is_empty(), "TOTP secret must not be empty");
        assert!(
            url.starts_with("otpauth://totp/"),
            "TOTP URL should start with otpauth://totp/, got: {url}"
        );
        assert!(
            url.contains("ServerBee"),
            "TOTP URL should contain issuer 'ServerBee'"
        );
        assert!(!qr.is_empty(), "QR code base64 must not be empty");
    }

    // ── DB integration tests ──────────────────────────────────────────────────

    use crate::test_utils::setup_test_db;

    #[tokio::test]
    async fn test_create_user_success() {
        let (db, _tmp) = setup_test_db().await;
        let user = AuthService::create_user(&db, "alice", "password123", "admin")
            .await
            .expect("create_user should succeed");
        assert_eq!(user.username, "alice");
        assert_eq!(user.role, "admin");
        assert!(!user.id.is_empty());
    }

    #[tokio::test]
    async fn test_create_user_duplicate() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "alice", "password123", "admin")
            .await
            .expect("first create should succeed");
        let result = AuthService::create_user(&db, "alice", "other_pass", "member").await;
        assert!(result.is_err(), "duplicate username should return an error");
    }

    #[tokio::test]
    async fn test_login_success() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "bob", "secret123", "member")
            .await
            .expect("create_user should succeed");
        let (session, user) = AuthService::login(&db, login_params("bob", "secret123"))
            .await
            .expect("login should succeed");
        assert_eq!(user.username, "bob");
        assert!(!session.token.is_empty());
    }

    #[tokio::test]
    async fn test_login_wrong_password() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "carol", "correct_pass", "member")
            .await
            .expect("create_user should succeed");
        let result = AuthService::login(&db, login_params("carol", "wrong_pass")).await;
        assert!(result.is_err(), "wrong password should return an error");
    }

    #[tokio::test]
    async fn test_login_nonexistent_user() {
        let (db, _tmp) = setup_test_db().await;
        let result = AuthService::login(&db, login_params("nobody", "pass")).await;
        assert!(
            result.is_err(),
            "logging in as nonexistent user should error"
        );
    }

    #[tokio::test]
    async fn test_validate_session_valid() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "dave", "pass1234", "member")
            .await
            .expect("create_user should succeed");
        let (session, _user) = AuthService::login(&db, login_params("dave", "pass1234"))
            .await
            .expect("login should succeed");
        let validated = AuthService::validate_session(&db, &session.token, 3600)
            .await
            .expect("validate_session should not error");
        assert!(validated.is_some(), "valid token should return a user");
        let (user, sess) = validated.unwrap();
        assert_eq!(user.username, "dave");
        assert_eq!(sess.source, "web");
    }

    #[tokio::test]
    async fn test_validate_session_invalid_token() {
        let (db, _tmp) = setup_test_db().await;
        let result = AuthService::validate_session(&db, "fake_token_that_does_not_exist", 3600)
            .await
            .expect("validate_session should not error");
        assert!(result.is_none(), "invalid token should return None");
    }

    #[tokio::test]
    async fn test_create_and_validate_api_key() {
        let (db, _tmp) = setup_test_db().await;
        let user = AuthService::create_user(&db, "eve", "pass5678", "admin")
            .await
            .expect("create_user should succeed");
        let (_model, raw_key) = AuthService::create_api_key(&db, &user.id, "my-key")
            .await
            .expect("create_api_key should succeed");
        assert!(
            raw_key.starts_with("serverbee_"),
            "raw key should start with serverbee_"
        );

        let validated = AuthService::validate_api_key(&db, &raw_key)
            .await
            .expect("validate_api_key should not error");
        assert!(validated.is_some(), "valid api key should return a user");
        assert_eq!(validated.unwrap().username, "eve");
    }

    #[tokio::test]
    async fn test_validate_api_key_invalid() {
        let (db, _tmp) = setup_test_db().await;
        let result = AuthService::validate_api_key(&db, "serverbee_totally_fake_key_here_xyz")
            .await
            .expect("validate_api_key should not error");
        assert!(result.is_none(), "invalid api key should return None");
    }

    #[tokio::test]
    async fn test_change_password_wrong_old() {
        let (db, _tmp) = setup_test_db().await;
        let user = AuthService::create_user(&db, "frank", "real_pass", "member")
            .await
            .expect("create_user should succeed");
        let result =
            AuthService::change_password(&db, &user.id, "wrong_old_pass", "new_pass123", None)
                .await;
        assert!(result.is_err(), "wrong old password should return an error");
    }

    #[tokio::test]
    async fn test_change_password_success() {
        let (db, _tmp) = setup_test_db().await;
        let user = AuthService::create_user(&db, "grace", "old_pass1", "member")
            .await
            .expect("create_user should succeed");
        AuthService::change_password(&db, &user.id, "old_pass1", "new_pass99", None)
            .await
            .expect("change_password should succeed");
        // Login with new password should succeed
        let result = AuthService::login(&db, login_params("grace", "new_pass99")).await;
        assert!(result.is_ok(), "login with new password should succeed");
        // Login with old password should fail
        let result2 = AuthService::login(&db, login_params("grace", "old_pass1")).await;
        assert!(result2.is_err(), "login with old password should fail");
    }

    #[tokio::test]
    async fn test_change_password_revokes_other_sessions() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "heidi", "old_pass1", "member")
            .await
            .expect("create_user should succeed");
        // Two active sessions (e.g. two browsers logged in).
        let (keep, _u) = AuthService::login(&db, login_params("heidi", "old_pass1"))
            .await
            .expect("login should succeed");
        let (other, _u) = AuthService::login(&db, login_params("heidi", "old_pass1"))
            .await
            .expect("login should succeed");

        AuthService::change_password(
            &db,
            &keep.user_id,
            "old_pass1",
            "new_pass123",
            Some(&keep.token),
        )
        .await
        .expect("change_password should succeed");

        // The caller's own session is preserved...
        let kept = AuthService::validate_session(&db, &keep.token, 3600)
            .await
            .expect("validate_session should not error");
        assert!(
            kept.is_some(),
            "the kept session must survive the password change"
        );
        // ...while every other session is revoked.
        let revoked = AuthService::validate_session(&db, &other.token, 3600)
            .await
            .expect("validate_session should not error");
        assert!(
            revoked.is_none(),
            "other sessions must be revoked after a password change"
        );
    }

    #[tokio::test]
    async fn test_change_password_without_keep_token_revokes_all_sessions() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "ivan", "old_pass1", "member")
            .await
            .expect("create_user should succeed");
        let (sess, _u) = AuthService::login(&db, login_params("ivan", "old_pass1"))
            .await
            .expect("login should succeed");

        // No keep token (e.g. API-key authenticated caller) -> revoke all.
        AuthService::change_password(&db, &sess.user_id, "old_pass1", "new_pass123", None)
            .await
            .expect("change_password should succeed");

        let validated = AuthService::validate_session(&db, &sess.token, 3600)
            .await
            .expect("validate_session should not error");
        assert!(
            validated.is_none(),
            "with no keep token, all sessions must be revoked"
        );
    }

    #[tokio::test]
    async fn test_init_admin_creates_random_admin_with_flag() {
        let (db, _tmp) = setup_test_db().await;
        let generated = AuthService::init_admin(&db)
            .await
            .expect("init_admin should succeed");
        let pwd = generated.expect("first run must return a generated password");
        assert!(!pwd.is_empty(), "generated password must not be empty");

        let admin = user::Entity::find()
            .filter(user::Column::Username.eq(AuthService::DEFAULT_ADMIN_USERNAME))
            .one(&db)
            .await
            .expect("query should succeed")
            .expect("admin user must exist");
        assert_eq!(admin.role, "admin");
        assert!(
            admin.must_change_password,
            "freshly seeded admin must require password change"
        );
        assert!(
            AuthService::verify_password(&pwd, &admin.password_hash).expect("verify"),
            "generated password must match stored hash"
        );
    }

    async fn seed_must_change_admin(db: &DatabaseConnection) -> user::Model {
        let u = AuthService::create_user(db, "admin", "init-pass-123", "admin")
            .await
            .expect("seed admin");
        let mut a: user::ActiveModel = u.into();
        a.must_change_password = Set(true);
        a.update(db).await.expect("set flag")
    }

    #[tokio::test]
    async fn test_complete_onboarding_success_password_only() {
        let (db, _tmp) = setup_test_db().await;
        let admin = seed_must_change_admin(&db).await;
        AuthService::complete_onboarding(&db, &admin.id, "brand-new-pass-9", None)
            .await
            .expect("onboarding should succeed");
        let after = user::Entity::find_by_id(&admin.id).one(&db).await.unwrap().unwrap();
        assert!(!after.must_change_password, "flag must be cleared");
        assert_eq!(after.username, "admin", "username unchanged when None");
        assert!(AuthService::verify_password("brand-new-pass-9", &after.password_hash).unwrap());
    }

    #[tokio::test]
    async fn test_complete_onboarding_with_username_change() {
        let (db, _tmp) = setup_test_db().await;
        let admin = seed_must_change_admin(&db).await;
        AuthService::complete_onboarding(&db, &admin.id, "np-12345", Some("  newname  "))
            .await
            .expect("should succeed and trim username");
        let after = user::Entity::find_by_id(&admin.id).one(&db).await.unwrap().unwrap();
        assert_eq!(after.username, "newname", "username trimmed + applied");
        assert!(!after.must_change_password);
    }

    #[tokio::test]
    async fn test_complete_onboarding_blank_username_is_ignored() {
        let (db, _tmp) = setup_test_db().await;
        let admin = seed_must_change_admin(&db).await;
        AuthService::complete_onboarding(&db, &admin.id, "np-12345", Some("   "))
            .await
            .expect("blank username treated as not provided");
        let after = user::Entity::find_by_id(&admin.id).one(&db).await.unwrap().unwrap();
        assert_eq!(after.username, "admin");
    }

    #[tokio::test]
    async fn test_complete_onboarding_rejects_same_password() {
        let (db, _tmp) = setup_test_db().await;
        let admin = seed_must_change_admin(&db).await;
        let r = AuthService::complete_onboarding(&db, &admin.id, "init-pass-123", None).await;
        assert!(r.is_err(), "reusing current password must be rejected");
    }

    #[tokio::test]
    async fn test_complete_onboarding_rejects_empty_password() {
        let (db, _tmp) = setup_test_db().await;
        let admin = seed_must_change_admin(&db).await;
        let r = AuthService::complete_onboarding(&db, &admin.id, "", None).await;
        assert!(r.is_err(), "empty password must be rejected");
    }

    #[tokio::test]
    async fn test_complete_onboarding_rejects_weak_password() {
        let (db, _tmp) = setup_test_db().await;
        let admin = seed_must_change_admin(&db).await;
        let r = AuthService::complete_onboarding(&db, &admin.id, "123", None).await;
        assert!(
            matches!(r, Err(AppError::Validation(_))),
            "short/weak password must be rejected with a validation error, got {r:?}"
        );
        let after = user::Entity::find_by_id(&admin.id).one(&db).await.unwrap().unwrap();
        assert!(
            after.must_change_password,
            "onboarding must not complete with a weak password"
        );
    }

    #[tokio::test]
    async fn test_change_password_rejects_weak_password() {
        let (db, _tmp) = setup_test_db().await;
        let user = AuthService::create_user(&db, "weakp", "old_pass1", "member")
            .await
            .expect("create user");
        let r = AuthService::change_password(&db, &user.id, "old_pass1", "123", None).await;
        assert!(
            matches!(r, Err(AppError::Validation(_))),
            "weak new password must be rejected, got {r:?}"
        );
        let after = user::Entity::find_by_id(&user.id).one(&db).await.unwrap().unwrap();
        assert!(
            AuthService::verify_password("old_pass1", &after.password_hash).unwrap(),
            "password must remain unchanged when new one is rejected"
        );
    }

    #[tokio::test]
    async fn test_complete_onboarding_rejects_when_flag_not_set() {
        let (db, _tmp) = setup_test_db().await;
        let u = AuthService::create_user(&db, "admin", "p", "admin").await.unwrap();
        let r = AuthService::complete_onboarding(&db, &u.id, "new-pass-1", None).await;
        assert!(r.is_err(), "onboarding when flag is false must be rejected");
    }

    #[tokio::test]
    async fn test_complete_onboarding_rejects_duplicate_username() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "taken", "p", "member").await.unwrap();
        let admin = seed_must_change_admin(&db).await;
        let r = AuthService::complete_onboarding(&db, &admin.id, "new-pass-1", Some("taken")).await;
        assert!(r.is_err(), "duplicate username must be rejected");
    }

    #[tokio::test]
    async fn validate_agent_token_rejects_pending_server() {
        use crate::entity::server;
        use serverbee_common::constants::CAP_DEFAULT;

        let (db, _tmp) = setup_test_db().await;
        let now = Utc::now();
        let sid = Uuid::new_v4().to_string();

        server::ActiveModel {
            id: Set(sid.clone()),
            token_hash: Set(None),
            token_prefix: Set(None),
            name: Set("pending".into()),
            cpu_name: Set(None),
            cpu_cores: Set(None),
            cpu_arch: Set(None),
            os: Set(None),
            kernel_version: Set(None),
            mem_total: Set(None),
            swap_total: Set(None),
            disk_total: Set(None),
            ipv4: Set(None),
            ipv6: Set(None),
            region: Set(None),
            country_code: Set(None),
            virtualization: Set(None),
            agent_version: Set(None),
            group_id: Set(None),
            weight: Set(0),
            hidden: Set(false),
            remark: Set(None),
            public_remark: Set(None),
            price: Set(None),
            billing_cycle: Set(None),
            currency: Set(None),
            expired_at: Set(None),
            traffic_limit: Set(None),
            traffic_limit_type: Set(None),
            billing_start_day: Set(None),
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(1),
            features: Set("[]".into()),
            last_remote_addr: Set(None),
            fingerprint: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        // Any plausible token string must not match a pending server.
        let result = AuthService::validate_agent_token(&db, "anything-here")
            .await
            .unwrap();
        assert!(result.is_none(), "pending server must not validate any token");
    }

    #[tokio::test]
    async fn validate_agent_token_rejects_row_with_prefix_but_null_hash() {
        // Defense-in-depth: if a future schema regression ever produced a row
        // with a non-NULL token_prefix but NULL token_hash, the query-level
        // `is_not_null()` filter must keep the row out of the candidate set.
        use serverbee_common::constants::CAP_DEFAULT;

        let (db, _tmp) = setup_test_db().await;
        let now = Utc::now();
        let sid = Uuid::new_v4().to_string();

        server::ActiveModel {
            id: Set(sid.clone()),
            token_hash: Set(None),
            token_prefix: Set(Some("testtest".into())),
            name: Set("half-bound".into()),
            cpu_name: Set(None),
            cpu_cores: Set(None),
            cpu_arch: Set(None),
            os: Set(None),
            kernel_version: Set(None),
            mem_total: Set(None),
            swap_total: Set(None),
            disk_total: Set(None),
            ipv4: Set(None),
            ipv6: Set(None),
            region: Set(None),
            country_code: Set(None),
            virtualization: Set(None),
            agent_version: Set(None),
            group_id: Set(None),
            weight: Set(0),
            hidden: Set(false),
            remark: Set(None),
            public_remark: Set(None),
            price: Set(None),
            billing_cycle: Set(None),
            currency: Set(None),
            expired_at: Set(None),
            traffic_limit: Set(None),
            traffic_limit_type: Set(None),
            billing_start_day: Set(None),
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(1),
            features: Set("[]".into()),
            last_remote_addr: Set(None),
            fingerprint: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        // Token whose 8-char prefix matches the row's `token_prefix`.
        let result = AuthService::validate_agent_token(&db, "testtest-not-a-real-token")
            .await
            .unwrap();
        assert!(
            result.is_none(),
            "row with non-NULL prefix but NULL token_hash must not validate"
        );
    }

    #[tokio::test]
    async fn test_init_admin_noop_when_users_exist() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "someone", "pass1234", "admin")
            .await
            .expect("seed a user");
        let generated = AuthService::init_admin(&db)
            .await
            .expect("init_admin should succeed");
        assert!(
            generated.is_none(),
            "init_admin must be a no-op when users already exist"
        );
    }
}
