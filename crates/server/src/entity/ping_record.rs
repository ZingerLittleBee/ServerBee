use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "ping_records")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub task_id: String,
    pub server_id: String,
    pub latency: f64,
    pub success: bool,
    pub error: Option<String>,
    pub time: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
