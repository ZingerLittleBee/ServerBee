use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = NetworkProbeRecord)]
#[sea_orm(table_name = "network_probe_record")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub server_id: String,
    pub target_id: String,
    pub avg_latency: Option<f64>,
    pub min_latency: Option<f64>,
    pub max_latency: Option<f64>,
    pub packet_loss: f64,
    pub packet_sent: i32,
    pub packet_received: i32,
    #[schema(value_type = String, format = DateTime)]
    pub timestamp: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
