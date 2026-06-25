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
        AuthService::validate_password_strength(password)?;
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

        let now = Utc::now();
        let password_reset = input.password.is_some();
        if let Some(ref password) = input.password {
            AuthService::validate_password_strength(password)?;
            let new_hash = AuthService::hash_password(password)?;
            active.password_hash = Set(new_hash);
            // Stamp the reset so mobile refresh rejects refresh tokens whose
            // session was issued earlier (the authoritative kill-switch).
            active.password_changed_at = Set(Some(now));
        }
        active.updated_at = Set(now);

        // If an admin reset this user's password, revoke all their existing
        // sessions so a previously issued (possibly stolen) session cannot
        // outlive the reset. This includes the mobile auth path, whose refresh
        // secret lives in `mobile_session` (a separate table); an admin reset
        // unconditionally drops all of the target user's mobile sessions. The
        // update + revocation run in one transaction so the reset can't commit
        // while sessions stay live.
        let updated = if password_reset {
            let txn = db.begin().await?;
            let updated = active.update(&txn).await?;
            session::Entity::delete_many()
                .filter(session::Column::UserId.eq(id))
                .exec(&txn)
                .await?;
            AuthService::revoke_user_mobile_sessions(&txn, id, None).await?;
            txn.commit().await?;
            updated
        } else {
            active.update(db).await?
        };

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

    #[tokio::test]
    async fn test_update_user_password_reset_revokes_sessions() {
        use crate::service::auth::{AuthService, LoginParams};

        let (db, _tmp) = setup_test_db().await;
        let user = UserService::create_user(&db, "reset_target", "old_pass1", "member")
            .await
            .expect("create user should succeed");
        // An active session for the user.
        let (sess, _u) = AuthService::login(
            &db,
            LoginParams {
                username: "reset_target",
                password: "old_pass1",
                totp_code: None,
                ip: "127.0.0.1",
                user_agent: "test",
                session_ttl: 3600,
            },
        )
        .await
        .expect("login should succeed");

        // Admin resets the password.
        UserService::update_user(
            &db,
            &user.id,
            UpdateUserInput {
                role: None,
                password: Some("new_pass123".to_string()),
            },
        )
        .await
        .expect("update_user should succeed");

        // The pre-existing session must be revoked by the reset.
        let validated = AuthService::validate_session(&db, &sess.token, 3600)
            .await
            .expect("validate_session should not error");
        assert!(
            validated.is_none(),
            "an admin password reset must revoke the user's existing sessions"
        );
    }

    #[tokio::test]
    async fn test_get_user_success() {
        // get_user returns the stored model for an existing id.
        let (db, _tmp) = setup_test_db().await;
        let created = UserService::create_user(&db, "lookup_me", "password1", "member")
            .await
            .expect("create user should succeed");

        let fetched = UserService::get_user(&db, &created.id)
            .await
            .expect("get_user should succeed for an existing id");
        assert_eq!(fetched.id, created.id, "fetched id must match created id");
        assert_eq!(fetched.username, "lookup_me", "username must match");
        assert_eq!(fetched.role, "member", "role must match");
    }

    #[tokio::test]
    async fn test_get_user_not_found() {
        // get_user returns NotFound for an id that does not exist.
        let (db, _tmp) = setup_test_db().await;
        let result = UserService::get_user(&db, "nonexistent-id").await;
        assert!(
            matches!(result, Err(AppError::NotFound(_))),
            "get_user must return NotFound for a missing id, got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_create_user_invalid_role_rejected() {
        // create_user rejects roles other than 'admin'/'member' before hitting the DB.
        let (db, _tmp) = setup_test_db().await;
        let result = UserService::create_user(&db, "bad_role", "password1", "superuser").await;
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "invalid role must be rejected with a validation error, got {result:?}"
        );
        // The user must not have been persisted.
        let users = UserService::list_users(&db).await.expect("list_users should succeed");
        assert!(users.is_empty(), "no user should be created when role is invalid");
    }

    #[tokio::test]
    async fn test_create_user_weak_password_rejected() {
        // create_user enforces the minimum password strength policy.
        let (db, _tmp) = setup_test_db().await;
        let result = UserService::create_user(&db, "weakpw", "123", "member").await;
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "a too-short password must be rejected, got {result:?}"
        );
        let users = UserService::list_users(&db).await.expect("list_users should succeed");
        assert!(users.is_empty(), "no user should be created with a weak password");
    }

    #[tokio::test]
    async fn test_create_user_success_member_role() {
        // create_user persists a valid member user with the requested role.
        let (db, _tmp) = setup_test_db().await;
        let created = UserService::create_user(&db, "valid_member", "password1", "member")
            .await
            .expect("create_user should succeed for a valid member");
        assert_eq!(created.username, "valid_member");
        assert_eq!(created.role, "member", "role must be persisted as member");
        assert!(created.totp_secret.is_none(), "new user must not have 2FA enabled");
    }

    #[tokio::test]
    async fn test_update_user_not_found() {
        // update_user returns NotFound when the target id does not exist.
        let (db, _tmp) = setup_test_db().await;
        let result = UserService::update_user(
            &db,
            "missing-id",
            UpdateUserInput {
                role: Some("admin".to_string()),
                password: None,
            },
        )
        .await;
        assert!(
            matches!(result, Err(AppError::NotFound(_))),
            "updating a missing user must return NotFound, got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_update_user_invalid_role_rejected() {
        // update_user rejects an invalid role value before mutating anything.
        let (db, _tmp) = setup_test_db().await;
        let user = UserService::create_user(&db, "role_target", "password1", "member")
            .await
            .expect("create user should succeed");

        let result = UserService::update_user(
            &db,
            &user.id,
            UpdateUserInput {
                role: Some("root".to_string()),
                password: None,
            },
        )
        .await;
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "an invalid role must be rejected with a validation error, got {result:?}"
        );
        // The role must remain unchanged.
        let after = UserService::get_user(&db, &user.id).await.expect("get_user should succeed");
        assert_eq!(after.role, "member", "role must be unchanged after rejection");
    }

    #[tokio::test]
    async fn test_update_user_demote_last_admin_blocked() {
        // Demoting the sole admin to member must be refused with a BadRequest.
        let (db, _tmp) = setup_test_db().await;
        let admin = UserService::create_user(&db, "only_admin", "password1", "admin")
            .await
            .expect("create admin should succeed");

        let result = UserService::update_user(
            &db,
            &admin.id,
            UpdateUserInput {
                role: Some("member".to_string()),
                password: None,
            },
        )
        .await;
        assert!(
            matches!(result, Err(AppError::BadRequest(_))),
            "demoting the last admin must be blocked, got {result:?}"
        );
        // The admin must still be an admin.
        let after = UserService::get_user(&db, &admin.id).await.expect("get_user should succeed");
        assert_eq!(after.role, "admin", "last admin must keep its admin role");
    }

    #[tokio::test]
    async fn test_update_user_demote_admin_allowed_when_multiple() {
        // Demoting an admin is allowed when another admin remains.
        let (db, _tmp) = setup_test_db().await;
        UserService::create_user(&db, "admin_keep", "password1", "admin")
            .await
            .expect("create first admin should succeed");
        let admin2 = UserService::create_user(&db, "admin_demote", "password2", "admin")
            .await
            .expect("create second admin should succeed");

        let updated = UserService::update_user(
            &db,
            &admin2.id,
            UpdateUserInput {
                role: Some("member".to_string()),
                password: None,
            },
        )
        .await
        .expect("demoting an admin must succeed when another admin exists");
        assert_eq!(updated.role, "member", "second admin should be demoted to member");
    }

    #[tokio::test]
    async fn test_update_user_admin_to_admin_skips_guard() {
        // Setting an existing admin's role to 'admin' must not trip the last-admin guard.
        let (db, _tmp) = setup_test_db().await;
        let admin = UserService::create_user(&db, "stay_admin", "password1", "admin")
            .await
            .expect("create admin should succeed");

        let updated = UserService::update_user(
            &db,
            &admin.id,
            UpdateUserInput {
                role: Some("admin".to_string()),
                password: None,
            },
        )
        .await
        .expect("setting admin->admin must succeed even as the sole admin");
        assert_eq!(updated.role, "admin", "role must remain admin");
    }

    #[tokio::test]
    async fn test_update_user_password_only_changes_hash_and_stamp() {
        // A password-only update rewrites the hash and stamps password_changed_at.
        let (db, _tmp) = setup_test_db().await;
        let user = UserService::create_user(&db, "pw_only", "old_pass1", "member")
            .await
            .expect("create user should succeed");
        assert!(user.password_changed_at.is_none(), "fresh user has no reset stamp");
        let original_hash = user.password_hash.clone();

        let updated = UserService::update_user(
            &db,
            &user.id,
            UpdateUserInput {
                role: None,
                password: Some("brand_new_pw9".to_string()),
            },
        )
        .await
        .expect("password-only update should succeed");

        assert_ne!(
            updated.password_hash, original_hash,
            "password hash must change after a reset"
        );
        assert!(
            updated.password_changed_at.is_some(),
            "password_changed_at must be stamped on a reset"
        );
        assert_eq!(updated.role, "member", "role must be unchanged on a password-only update");
        // The new password must verify against the stored hash.
        assert!(
            crate::service::auth::AuthService::verify_password("brand_new_pw9", &updated.password_hash)
                .expect("verify_password should succeed"),
            "new password must verify against the updated hash"
        );
    }

    #[tokio::test]
    async fn test_update_user_password_reset_weak_password_rejected() {
        // A weak reset password is rejected without revoking sessions or changing the hash.
        let (db, _tmp) = setup_test_db().await;
        let user = UserService::create_user(&db, "weak_reset", "old_pass1", "member")
            .await
            .expect("create user should succeed");
        let original_hash = user.password_hash.clone();

        let result = UserService::update_user(
            &db,
            &user.id,
            UpdateUserInput {
                role: None,
                password: Some("123".to_string()),
            },
        )
        .await;
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "a weak reset password must be rejected, got {result:?}"
        );
        let after = UserService::get_user(&db, &user.id).await.expect("get_user should succeed");
        assert_eq!(
            after.password_hash, original_hash,
            "password hash must be unchanged when the reset is rejected"
        );
        assert!(
            after.password_changed_at.is_none(),
            "password_changed_at must not be stamped on a rejected reset"
        );
    }

    #[tokio::test]
    async fn test_update_user_role_only_keeps_sessions() {
        // A role-only update (no password) must NOT revoke the user's sessions.
        use crate::service::auth::{AuthService, LoginParams};

        let (db, _tmp) = setup_test_db().await;
        // Two admins so the demoted one can be moved without tripping the guard.
        UserService::create_user(&db, "guard_admin", "password1", "admin")
            .await
            .expect("create first admin should succeed");
        let target = UserService::create_user(&db, "role_sess", "password2", "admin")
            .await
            .expect("create second admin should succeed");
        let (sess, _u) = AuthService::login(
            &db,
            LoginParams {
                username: "role_sess",
                password: "password2",
                totp_code: None,
                ip: "127.0.0.1",
                user_agent: "test",
                session_ttl: 3600,
            },
        )
        .await
        .expect("login should succeed");

        UserService::update_user(
            &db,
            &target.id,
            UpdateUserInput {
                role: Some("member".to_string()),
                password: None,
            },
        )
        .await
        .expect("role-only update should succeed");

        // The session must survive a role-only update (no password reset path).
        let validated = AuthService::validate_session(&db, &sess.token, 3600)
            .await
            .expect("validate_session should not error");
        assert!(
            validated.is_some(),
            "a role-only update must not revoke the user's sessions"
        );
    }

    #[tokio::test]
    async fn test_delete_user_not_found() {
        // delete_user returns NotFound for an unknown id.
        let (db, _tmp) = setup_test_db().await;
        let result = UserService::delete_user(&db, "ghost-id").await;
        assert!(
            matches!(result, Err(AppError::NotFound(_))),
            "deleting a missing user must return NotFound, got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_delete_admin_allowed_when_multiple() {
        // Deleting an admin is permitted when another admin remains.
        let (db, _tmp) = setup_test_db().await;
        let keep = UserService::create_user(&db, "admin_survivor", "password1", "admin")
            .await
            .expect("create first admin should succeed");
        let doomed = UserService::create_user(&db, "admin_doomed", "password2", "admin")
            .await
            .expect("create second admin should succeed");

        UserService::delete_user(&db, &doomed.id)
            .await
            .expect("deleting an admin must succeed when another admin exists");

        // The deleted admin is gone; the survivor remains.
        assert!(
            UserService::get_user(&db, &doomed.id).await.is_err(),
            "deleted admin must no longer exist"
        );
        assert!(
            UserService::get_user(&db, &keep.id).await.is_ok(),
            "surviving admin must still exist"
        );
    }

    #[tokio::test]
    async fn test_delete_user_removes_oauth_accounts() {
        // delete_user cascades to oauth_account rows owned by the user.
        let (db, _tmp) = setup_test_db().await;
        // Keep an extra admin so the deletion guard is not triggered.
        UserService::create_user(&db, "oauth_admin", "password1", "admin")
            .await
            .expect("create admin should succeed");
        let user = UserService::create_user(&db, "oauth_user", "password2", "member")
            .await
            .expect("create user should succeed");

        let now = Utc::now();
        oauth_account::ActiveModel {
            id: Set("oa-1".to_string()),
            user_id: Set(user.id.clone()),
            provider: Set("github".to_string()),
            provider_user_id: Set("gh-123".to_string()),
            email: Set(None),
            display_name: Set(None),
            created_at: Set(now),
        }
        .insert(&db)
        .await
        .expect("seeding an oauth_account should succeed");

        UserService::delete_user(&db, &user.id)
            .await
            .expect("delete_user should succeed");

        let remaining = oauth_account::Entity::find()
            .filter(oauth_account::Column::UserId.eq(&user.id))
            .all(&db)
            .await
            .expect("query should succeed");
        assert!(remaining.is_empty(), "oauth accounts must be removed with the user");
    }

    #[test]
    fn test_user_response_from_with_2fa() {
        use chrono::TimeZone;
        // UserResponse::from maps fields and reports has_2fa=true when a TOTP secret is set.
        let created = chrono::Utc
            .with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
            .unwrap();
        let updated = chrono::Utc
            .with_ymd_and_hms(2026, 1, 2, 6, 7, 8)
            .unwrap();
        let model = user::Model {
            id: "u-1".to_string(),
            username: "withtfa".to_string(),
            password_hash: "hash".to_string(),
            role: "admin".to_string(),
            totp_secret: Some("SECRET".to_string()),
            must_change_password: false,
            password_changed_at: None,
            created_at: created,
            updated_at: updated,
        };
        let resp = UserResponse::from(model);
        assert_eq!(resp.id, "u-1");
        assert_eq!(resp.username, "withtfa");
        assert_eq!(resp.role, "admin");
        assert!(resp.has_2fa, "has_2fa must be true when totp_secret is set");
        assert_eq!(resp.created_at, created, "created_at must be mapped verbatim");
        assert_eq!(resp.updated_at, updated, "updated_at must be mapped verbatim");
    }

    #[test]
    fn test_user_response_from_without_2fa() {
        use chrono::TimeZone;
        // UserResponse::from reports has_2fa=false when no TOTP secret is set.
        let ts = chrono::Utc.with_ymd_and_hms(2026, 6, 25, 12, 0, 0).unwrap();
        let model = user::Model {
            id: "u-2".to_string(),
            username: "no2fa".to_string(),
            password_hash: "hash".to_string(),
            role: "member".to_string(),
            totp_secret: None,
            must_change_password: false,
            password_changed_at: None,
            created_at: ts,
            updated_at: ts,
        };
        let resp = UserResponse::from(model);
        assert!(!resp.has_2fa, "has_2fa must be false when totp_secret is None");
        assert_eq!(resp.role, "member");
    }

    #[test]
    fn test_default_role_is_member() {
        // The serde default role helper returns 'member'.
        assert_eq!(default_role(), "member", "default role must be 'member'");
    }
}
