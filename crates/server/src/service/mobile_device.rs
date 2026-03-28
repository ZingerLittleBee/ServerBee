use chrono::Utc;
use sea_orm::prelude::Expr;
use sea_orm::*;
use uuid::Uuid;

use crate::entity::mobile_device_registration;
use crate::error::AppError;

pub struct MobileDeviceService;

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct MobileDeviceState {
    pub installation_id: String,
    pub platform: String,
    pub push_token: Option<String>,
    pub app_version: String,
    pub locale: String,
    pub permission_status: String,
    pub firing_alerts_push: bool,
    pub resolved_alerts_push: bool,
    pub disabled: bool,
    pub last_seen_at: Option<String>,
    pub registered: bool,
}

impl MobileDeviceService {
    /// Upsert device registration by installation_id.
    #[allow(clippy::too_many_arguments)]
    pub async fn register(
        db: &DatabaseConnection,
        user_id: &str,
        installation_id: &str,
        platform: &str,
        push_token: Option<&str>,
        app_version: &str,
        locale: &str,
        permission_status: &str,
        firing_alerts_push: bool,
        resolved_alerts_push: bool,
    ) -> Result<mobile_device_registration::Model, AppError> {
        let now = Utc::now();

        // Handle duplicate push token: deactivate old bindings
        if let Some(token) = push_token {
            mobile_device_registration::Entity::update_many()
                .filter(mobile_device_registration::Column::PushToken.eq(token))
                .filter(
                    mobile_device_registration::Column::InstallationId.ne(installation_id),
                )
                .col_expr(
                    mobile_device_registration::Column::PushToken,
                    Expr::value(Option::<String>::None),
                )
                .col_expr(
                    mobile_device_registration::Column::UpdatedAt,
                    Expr::value(now),
                )
                .exec(db)
                .await?;
        }

        // Upsert by installation_id
        let existing = mobile_device_registration::Entity::find()
            .filter(mobile_device_registration::Column::InstallationId.eq(installation_id))
            .one(db)
            .await?;

        if let Some(existing) = existing {
            let mut update: mobile_device_registration::ActiveModel = existing.into();
            update.user_id = Set(user_id.to_string());
            update.platform = Set(platform.to_string());
            update.push_token = Set(push_token.map(|s| s.to_string()));
            update.app_version = Set(app_version.to_string());
            update.locale = Set(locale.to_string());
            update.permission_status = Set(permission_status.to_string());
            update.firing_alerts_push = Set(firing_alerts_push);
            update.resolved_alerts_push = Set(resolved_alerts_push);
            update.disabled_at = Set(None);
            update.last_seen_at = Set(Some(now));
            update.updated_at = Set(now);
            Ok(update.update(db).await?)
        } else {
            let model = mobile_device_registration::ActiveModel {
                id: Set(Uuid::new_v4().to_string()),
                user_id: Set(user_id.to_string()),
                installation_id: Set(installation_id.to_string()),
                platform: Set(platform.to_string()),
                push_token: Set(push_token.map(|s| s.to_string())),
                provider: Set("expo".to_string()),
                app_version: Set(app_version.to_string()),
                locale: Set(locale.to_string()),
                permission_status: Set(permission_status.to_string()),
                firing_alerts_push: Set(firing_alerts_push),
                resolved_alerts_push: Set(resolved_alerts_push),
                disabled_at: Set(None),
                last_seen_at: Set(Some(now)),
                created_at: Set(now),
                updated_at: Set(now),
            };
            Ok(model.insert(db).await?)
        }
    }

    /// Unregister: clear push token and disable.
    pub async fn unregister(
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

    /// Verify that an installation belongs to the given user.
    pub async fn verify_ownership(
        db: &DatabaseConnection,
        installation_id: &str,
        user_id: &str,
    ) -> Result<(), AppError> {
        let device = mobile_device_registration::Entity::find()
            .filter(mobile_device_registration::Column::InstallationId.eq(installation_id))
            .one(db)
            .await?;
        match device {
            Some(d) if d.user_id != user_id => {
                Err(AppError::Forbidden("Device belongs to another user".to_string()))
            }
            _ => Ok(()),
        }
    }

    /// Get current device state, scoped to the authenticated user.
    pub async fn get_current_owned(
        db: &DatabaseConnection,
        installation_id: &str,
        user_id: &str,
    ) -> Result<MobileDeviceState, AppError> {
        let device = mobile_device_registration::Entity::find()
            .filter(mobile_device_registration::Column::InstallationId.eq(installation_id))
            .filter(mobile_device_registration::Column::UserId.eq(user_id))
            .one(db)
            .await?;

        Ok(match device {
            Some(d) => MobileDeviceState {
                installation_id: d.installation_id,
                platform: d.platform,
                push_token: d.push_token,
                app_version: d.app_version,
                locale: d.locale,
                permission_status: d.permission_status,
                firing_alerts_push: d.firing_alerts_push,
                resolved_alerts_push: d.resolved_alerts_push,
                disabled: d.disabled_at.is_some(),
                last_seen_at: d.last_seen_at.map(|t| t.to_rfc3339()),
                registered: true,
            },
            None => MobileDeviceState {
                installation_id: installation_id.to_string(),
                platform: String::new(),
                push_token: None,
                app_version: String::new(),
                locale: "en".to_string(),
                permission_status: "undetermined".to_string(),
                firing_alerts_push: true,
                resolved_alerts_push: false,
                disabled: false,
                last_seen_at: None,
                registered: false,
            },
        })
    }

    /// Get all active devices with push tokens for a given alert event type.
    pub async fn get_push_targets(
        db: &DatabaseConnection,
        is_firing: bool,
    ) -> Result<Vec<mobile_device_registration::Model>, AppError> {
        let mut query = mobile_device_registration::Entity::find()
            .filter(mobile_device_registration::Column::DisabledAt.is_null())
            .filter(mobile_device_registration::Column::PushToken.is_not_null());

        if is_firing {
            query =
                query.filter(mobile_device_registration::Column::FiringAlertsPush.eq(true));
        } else {
            query = query
                .filter(mobile_device_registration::Column::ResolvedAlertsPush.eq(true));
        }

        Ok(query.all(db).await?)
    }
}
