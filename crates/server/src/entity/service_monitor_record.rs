use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = ServiceMonitorRecord)]
#[sea_orm(table_name = "service_monitor_record")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub monitor_id: String,
    pub success: bool,
    pub latency: Option<f64>,
    pub detail_json: String,
    pub error: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub time: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
#[allow(dead_code)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
