use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = UnlockEvent)]
#[sea_orm(table_name = "unlock_event")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub server_id: String,
    pub service_id: String,
    pub old_status: String,
    pub new_status: String,
    #[schema(value_type = String, format = DateTime)]
    pub changed_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
