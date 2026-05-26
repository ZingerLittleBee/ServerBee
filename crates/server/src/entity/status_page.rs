use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = StatusPage)]
#[sea_orm(table_name = "status_page")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub server_ids_json: String,
    pub group_by_server_group: bool,
    pub enabled: bool,
    pub uptime_yellow_threshold: f64,
    pub uptime_red_threshold: f64,
    pub show_ip_quality: bool,
    pub default_layout: String,
    pub show_server_detail: bool,
    pub show_network: bool,
    pub show_incidents: bool,
    pub show_maintenance: bool,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
#[allow(dead_code)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
