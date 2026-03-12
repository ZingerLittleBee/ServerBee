use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "notifications")]
#[allow(dead_code)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub notify_type: String,
    pub config_json: String,
    pub enabled: bool,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
#[allow(dead_code)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
