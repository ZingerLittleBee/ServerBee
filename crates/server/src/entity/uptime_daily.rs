use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = UptimeDaily)]
#[sea_orm(table_name = "uptime_daily")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub server_id: String,
    pub date: Date,
    pub total_minutes: i32,
    pub online_minutes: i32,
    pub downtime_incidents: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
#[allow(dead_code)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
