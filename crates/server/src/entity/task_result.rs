use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = TaskResult)]
#[sea_orm(table_name = "task_results")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub task_id: String,
    pub server_id: String,
    pub output: String,
    pub exit_code: i32,
    pub run_id: Option<String>,
    pub attempt: i32,
    #[schema(value_type = Option<String>, format = DateTime)]
    pub started_at: Option<DateTimeUtc>,
    #[schema(value_type = String, format = DateTime)]
    pub finished_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
