use chrono::Utc;
use sea_orm::*;
use serde::{Deserialize, Serialize};

use crate::entity::{api_key, oauth_account, session, user};
use crate::error::AppError;
use crate::service::auth::AuthService;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub role: String,
    pub has_2fa: bool,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

impl From<user::Model> for UserResponse {
    fn from(u: user::Model) -> Self {
        Self {
            id: u.id,
            username: u.username,
            role: u.role,
            has_2fa: u.totp_secret.is_some(),
            created_at: u.created_at,
            updated_at: u.updated_at,
        }
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateUserInput {
    pub username: String,
    pub password: String,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "member".to_string()
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateUserInput {
    pub role: Option<String>,
    pub password: Option<String>,
}

pub struct UserService;

impl UserService {
    /// List all users ordered by created_at ascending.
    pub async fn list_users(db: &DatabaseConnection) -> Result<Vec<user::Model>, AppError> {
        let users = user::Entity::find()
            .order_by_asc(user::Column::CreatedAt)
            .all(db)
            .await?;
        Ok(users)
    }

    /// Get a single user by ID, returning NotFound if missing.
    pub async fn get_user(db: &DatabaseConnection, id: &str) -> Result<user::Model, AppError> {
        let user = user::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
        Ok(user)
    }

    /// Validate that a role string is one of the accepted values.
    fn validate_role(role: &str) -> Result<(), AppError> {
        if role != "admin" && role != "member" {
            return Err(AppError::Validation(format!(
                "Invalid role '{}', must be 'admin' or 'member'",
                role
            )));
        }
        Ok(())
    }

    /// Create a new user, delegating password hashing to AuthService.
    pub async fn create_user(
        db: &DatabaseConnection,
        username: &str,
        password: &str,
        role: &str,
    ) -> Result<user::Model, AppError> {
        Self::validate_role(role)?;
        if password.len() < 6 {
            return Err(AppError::Validation(
                "Password must be at least 6 characters".to_string(),
            ));
        }
        AuthService::create_user(db, username, password, role).await
    }

    /// Update a user's role and optionally reset their password.
    pub async fn update_user(
        db: &DatabaseConnection,
        id: &str,
        input: UpdateUserInput,
    ) -> Result<user::Model, AppError> {
        let user = user::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

        let mut active: user::ActiveModel = user.clone().into();

        if let Some(ref role) = input.role {
            Self::validate_role(role)?;
            // Guard: do not allow demoting the last admin
            if user.role == "admin" && role != "admin" {
                let admin_count = user::Entity::find()
                    .filter(user::Column::Role.eq("admin"))
                    .count(db)
                    .await?;
                if admin_count <= 1 {
                    return Err(AppError::BadRequest(
                        "Cannot demote the last admin user".to_string(),
                    ));
                }
            }
            active.role = Set(role.clone());
        }

        if let Some(ref password) = input.password {
            if password.len() < 6 {
                return Err(AppError::Validation(
                    "Password must be at least 6 characters".to_string(),
                ));
            }
            let new_hash = AuthService::hash_password(password)?;
            active.password_hash = Set(new_hash);
        }

        active.updated_at = Set(Utc::now());
        let updated = active.update(db).await?;
        Ok(updated)
    }

    /// Delete a user along with their sessions and API keys.
    /// Refuses to delete the last admin.
    pub async fn delete_user(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let user = user::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

        // Guard: do not allow deleting the last admin
        if user.role == "admin" {
            let admin_count = user::Entity::find()
                .filter(user::Column::Role.eq("admin"))
                .count(db)
                .await?;
            if admin_count <= 1 {
                return Err(AppError::BadRequest(
                    "Cannot delete the last admin user".to_string(),
                ));
            }
        }

        // Clean up sessions
        session::Entity::delete_many()
            .filter(session::Column::UserId.eq(id))
            .exec(db)
            .await?;

        // Clean up API keys
        api_key::Entity::delete_many()
            .filter(api_key::Column::UserId.eq(id))
            .exec(db)
            .await?;

        // Clean up OAuth accounts
        oauth_account::Entity::delete_many()
            .filter(oauth_account::Column::UserId.eq(id))
            .exec(db)
            .await?;

        // Delete the user
        user::Entity::delete_by_id(id).exec(db).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_db;

    #[tokio::test]
    async fn test_list_users() {
        let (db, _tmp) = setup_test_db().await;
        UserService::create_user(&db, "user_one", "password1", "admin")
            .await
            .expect("create user_one should succeed");
        UserService::create_user(&db, "user_two", "password2", "member")
            .await
            .expect("create user_two should succeed");

        let users = UserService::list_users(&db)
            .await
            .expect("list_users should succeed");
        assert_eq!(users.len(), 2, "should list exactly 2 users");
    }

    #[tokio::test]
    async fn test_delete_user_cascading() {
        let (db, _tmp) = setup_test_db().await;
        // Create an admin (so we have more than one admin / can delete member)
        let admin = UserService::create_user(&db, "admin_user", "admin_pass1", "admin")
            .await
            .expect("create admin should succeed");
        let member = UserService::create_user(&db, "member_user", "member_pass1", "member")
            .await
            .expect("create member should succeed");

        // Create an API key for the member
        use crate::service::auth::AuthService;
        AuthService::create_api_key(&db, &member.id, "member-key")
            .await
            .expect("create_api_key should succeed");

        // Delete the member
        UserService::delete_user(&db, &member.id)
            .await
            .expect("delete_user should succeed");

        // get_user for deleted member should now return NotFound
        let result = UserService::get_user(&db, &member.id).await;
        assert!(result.is_err(), "get_user for deleted user should error");

        // Admin should still exist
        let still_there = UserService::get_user(&db, &admin.id).await;
        assert!(still_there.is_ok(), "admin user should still exist");
    }

    #[tokio::test]
    async fn test_delete_last_admin_blocked() {
        let (db, _tmp) = setup_test_db().await;
        let admin = UserService::create_user(&db, "sole_admin", "admin_pass2", "admin")
            .await
            .expect("create sole_admin should succeed");

        let result = UserService::delete_user(&db, &admin.id).await;
        assert!(
            result.is_err(),
            "deleting the last admin should return an error"
        );
    }

    #[tokio::test]
    async fn test_update_role() {
        let (db, _tmp) = setup_test_db().await;
        // Need two admins so we can safely operate; start with one admin and one member
        UserService::create_user(&db, "admin_a", "admin_pass3", "admin")
            .await
            .expect("create admin_a should succeed");
        let member = UserService::create_user(&db, "member_b", "member_pass3", "member")
            .await
            .expect("create member_b should succeed");

        // Promote member to admin
        let updated = UserService::update_user(
            &db,
            &member.id,
            UpdateUserInput {
                role: Some("admin".to_string()),
                password: None,
            },
        )
        .await
        .expect("update_user should succeed");

        assert_eq!(updated.role, "admin", "member should now have admin role");
    }
}
