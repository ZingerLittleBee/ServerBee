use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = UnlockService)]
#[sea_orm(table_name = "unlock_service")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub key: String,
    pub name: String,
    pub category: String,
    pub popularity: i32,
    pub is_builtin: bool,
    pub enabled: bool,
    pub detector: Option<String>,
    /// JSON: custom request config
    pub request: Option<String>,
    /// JSON: custom match rules
    pub rules: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
