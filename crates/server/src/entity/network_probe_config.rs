use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = NetworkProbeConfig)]
#[sea_orm(table_name = "network_probe_config")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub server_id: String,
    pub target_id: String,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
