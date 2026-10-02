use sea_orm::entity::prelude::*;

/// Verified relay registrations are separate from legacy direct-APNs tokens.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mobile_push_registrations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub installation_id: String,
    pub user_id: String,
    pub mobile_session_id: String,
    pub revision: i64,
    pub enabled: bool,
    pub alerts: bool,
    pub security: bool,
    pub task_failure: bool,
    pub task_success: bool,
    pub device_token: Option<String>,
    pub environment: Option<String>,
    pub key_id: Option<String>,
    pub grant_id: Option<String>,
    pub grant_token: Option<String>,
    pub grant_expires_at: Option<DateTimeUtc>,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
