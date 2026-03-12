use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "alert_states")]
#[allow(dead_code)]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub rule_id: String,
    pub server_id: String,
    pub first_triggered_at: DateTimeUtc,
    pub last_notified_at: DateTimeUtc,
    pub count: i32,
    pub resolved: bool,
    pub resolved_at: Option<DateTimeUtc>,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
#[allow(dead_code)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
