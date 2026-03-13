use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = ServerGroup)]
#[sea_orm(table_name = "server_groups")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub name: String,
    pub weight: i32,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::server::Entity")]
    Servers,
}

impl Related<super::server::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Servers.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
