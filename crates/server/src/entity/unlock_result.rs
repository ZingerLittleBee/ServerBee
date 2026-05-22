use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = UnlockResult)]
#[sea_orm(table_name = "unlock_result")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub server_id: String,
    pub service_id: String,
    pub status: String,
    pub region: Option<String>,
    pub latency_ms: Option<i32>,
    pub detail: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub checked_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
