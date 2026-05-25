use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize)]
#[sea_orm(table_name = "spa_themes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub uuid: String,
    pub manifest_id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub manifest_json: String,
    #[serde(skip)]
    pub package_data: Vec<u8>,
    #[serde(skip)]
    pub preview_data: Option<Vec<u8>>,
    pub preview_mime: Option<String>,
    pub size_bytes: i64,
    pub uploaded_by: String,
    pub uploaded_at: DateTimeUtc,
    pub is_superseded: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
