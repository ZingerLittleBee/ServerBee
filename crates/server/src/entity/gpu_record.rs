use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = GpuRecord)]
#[sea_orm(table_name = "gpu_records")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub server_id: String,
    #[schema(value_type = String, format = DateTime)]
    pub time: DateTimeUtc,
    pub device_index: i32,
    pub device_name: String,
    pub mem_total: i64,
    pub mem_used: i64,
    pub utilization: f64,
    pub temperature: f64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
