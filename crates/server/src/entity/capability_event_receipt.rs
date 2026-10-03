use sea_orm::entity::prelude::*;

/// Admission tombstone, retained after the pending alert intent is consumed.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "capability_event_receipts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub server_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub msg_id: String,
    pub payload_hash: String,
    pub occurred_at: DateTimeUtc,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
