use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = ServiceMonitor)]
#[sea_orm(table_name = "service_monitor")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub monitor_type: String,
    pub target: String,
    pub interval: i32,
    pub config_json: String,
    pub notification_group_id: Option<String>,
    pub retry_count: i32,
    pub server_ids_json: Option<String>,
    pub enabled: bool,
    pub last_status: Option<bool>,
    pub consecutive_failures: i32,
    #[schema(value_type = Option<String>, format = DateTime)]
    pub last_checked_at: Option<DateTimeUtc>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
#[allow(dead_code)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
