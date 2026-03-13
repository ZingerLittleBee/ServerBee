use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = AlertRule)]
#[sea_orm(table_name = "alert_rules")]
#[allow(dead_code)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub rules_json: String,
    pub trigger_mode: String,
    pub notification_group_id: Option<String>,
    pub fail_trigger_tasks: Option<String>,
    pub recover_trigger_tasks: Option<String>,
    pub cover_type: String,
    pub server_ids_json: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
#[allow(dead_code)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
