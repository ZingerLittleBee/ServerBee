use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Text")]
pub enum SourceType {
    #[sea_orm(string_value = "Builtin")]
    Builtin,
    #[sea_orm(string_value = "Url")]
    Url,
    #[sea_orm(string_value = "Upload")]
    Upload,
    #[sea_orm(string_value = "BundledByTheme")]
    BundledByTheme,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "widget_module")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub version: String,
    pub source_type: SourceType,
    pub source_url: Option<String>,
    pub bundled_by_theme_id: Option<String>,
    pub manifest_json: String,
    pub code_sha256: String,
    pub entry_path: String,
    #[serde(skip)]
    pub package_blob: Option<Vec<u8>>,
    pub installed_by: Option<i64>,
    pub installed_at: DateTimeUtc,
    pub enabled: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
