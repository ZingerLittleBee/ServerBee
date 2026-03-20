use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = Maintenance)]
#[sea_orm(table_name = "maintenance")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub start_at: DateTimeUtc,
    #[schema(value_type = String, format = DateTime)]
    pub end_at: DateTimeUtc,
    pub server_ids_json: Option<String>,
    pub status_page_ids_json: Option<String>,
    pub active: bool,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
#[allow(dead_code)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
