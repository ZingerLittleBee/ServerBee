use sea_orm::entity::prelude::*;

/// Pending event metadata only, atomically captured with its producer update.
/// Removed in the same commit as state and encrypted installation jobs.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "alert_event_intents")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub rule_id: String,
    pub server_id: String,
    pub event_type: String,
    pub trigger_mode: String,
    pub first_triggered_at: DateTimeUtc,
    pub occurred_at: DateTimeUtc,
    pub count: i32,
    pub should_notify: bool,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
