use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "task_results")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub task_id: String,
    pub server_id: String,
    pub output: String,
    pub exit_code: i32,
    pub finished_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
