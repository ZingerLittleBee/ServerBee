use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use rand::RngCore;
use sea_orm::*;
use uuid::Uuid;

use crate::config::AdminConfig;
use crate::entity::{api_key, server, session, user};
use crate::error::AppError;

pub struct AuthService;

impl AuthService {
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
            created_at: Set(now),
            updated_at: Set(now),
        };

        let result = new_user.insert(db).await?;
        Ok(result)
    }

    /// Authenticate a user by username and password, creating a new session.
    /// Returns the session and user models on success.
    pub async fn login(
        db: &DatabaseConnection,
        username: &str,
        password: &str,
        ip: &str,
        user_agent: &str,
        session_ttl: i64,
    ) -> Result<(session::Model, user::Model), AppError> {
        let user = user::Entity::find()
            .filter(user::Column::Username.eq(username))
            .one(db)
            .await?
            .ok_or(AppError::Unauthorized)?;

        let valid = Self::verify_password(password, &user.password_hash)?;
        if !valid {
            return Err(AppError::Unauthorized);
        }

        let token = Self::generate_session_token();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(session_ttl);

        let new_session = session::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            user_id: Set(user.id.clone()),
            token: Set(token),
            ip: Set(ip.to_string()),
            user_agent: Set(user_agent.to_string()),
            expires_at: Set(expires_at),
            created_at: Set(now),
        };

        let session_model = new_session.insert(db).await?;
        Ok((session_model, user))
    }

    /// Validate a session token. If valid and not expired, extends the
    /// expiration (sliding expiry) and returns the associated user.
    pub async fn validate_session(
        db: &DatabaseConnection,
        token: &str,
        session_ttl: i64,
    ) -> Result<Option<user::Model>, AppError> {
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
            session::Entity::delete_by_id(&session.id)
                .exec(db)
                .await?;
            return Ok(None);
        }

        // Sliding expiry: extend expires_at
        let user_id = session.user_id.clone();
        let new_expires = Utc::now() + chrono::Duration::seconds(session_ttl);
        let mut active: session::ActiveModel = session.into();
        active.expires_at = Set(new_expires);
        active.update(db).await?;

        // Fetch the user
        let user = user::Entity::find_by_id(&user_id).one(db).await?;

        Ok(user)
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
    /// The key has the format "sb_" + random base64url bytes.
    /// Only the argon2 hash and a prefix (first 8 chars after "sb_") are stored.
    pub async fn create_api_key(
        db: &DatabaseConnection,
        user_id: &str,
        name: &str,
    ) -> Result<(api_key::Model, String), AppError> {
        let raw_key = Self::generate_api_key_raw();
        let after_prefix = &raw_key[3..]; // strip "sb_"
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
        if !key.starts_with("sb_") || key.len() < 11 {
            return Ok(None);
        }

        let after_prefix = &key[3..];
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
                let user = user::Entity::find_by_id(&candidate_user_id)
                    .one(db)
                    .await?;
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
            .all(db)
            .await?;

        for candidate in candidates {
            if Self::verify_password(token, &candidate.token_hash)? {
                return Ok(Some(candidate));
            }
        }

        Ok(None)
    }

    /// Initialize the admin user if the users table is empty.
    /// If the admin password is not configured, generates a random one and logs it.
    pub async fn init_admin(
        db: &DatabaseConnection,
        admin_config: &AdminConfig,
    ) -> Result<(), AppError> {
        let user_count = user::Entity::find().count(db).await?;

        if user_count > 0 {
            return Ok(());
        }

        let password = if admin_config.password.is_empty() {
            let generated = Self::generate_session_token();
            tracing::info!(
                "No admin password configured. Generated admin password: {}",
                generated
            );
            generated
        } else {
            admin_config.password.clone()
        };

        Self::create_user(db, &admin_config.username, &password, "admin").await?;
        tracing::info!("Admin user '{}' created", admin_config.username);

        Ok(())
    }

    /// Generate a cryptographically random session token (32 bytes, base64url encoded).
    pub fn generate_session_token() -> String {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Generate a raw API key: "sb_" + 32 random bytes (base64url encoded).
    pub fn generate_api_key_raw() -> String {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        format!("sb_{}", URL_SAFE_NO_PAD.encode(bytes))
    }

    /// Change a user's password after verifying the old password.
    pub async fn change_password(
        db: &DatabaseConnection,
        user_id: &str,
        old_password: &str,
        new_password: &str,
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

        let new_hash = Self::hash_password(new_password)?;
        let mut active: user::ActiveModel = user.into();
        active.password_hash = Set(new_hash);
        active.updated_at = Set(Utc::now());
        active.update(db).await?;

        Ok(())
    }
}
