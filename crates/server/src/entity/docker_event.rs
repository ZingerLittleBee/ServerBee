use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "docker_event")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub server_id: String,
    pub timestamp: i64,
    pub event_type: String,
    pub action: String,
    pub actor_id: String,
    pub actor_name: Option<String>,
    pub attributes: Option<String>,
    pub created_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
