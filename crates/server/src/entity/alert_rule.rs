use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "alert_rules")]
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
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
