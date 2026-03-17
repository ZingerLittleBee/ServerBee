use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "tasks")]
#[allow(dead_code)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub command: String,
    pub server_ids_json: String,
    pub created_by: String,
    pub task_type: String,
    pub name: Option<String>,
    pub cron_expression: Option<String>,
    pub enabled: bool,
    pub timeout: Option<i32>,
    pub retry_count: i32,
    pub retry_interval: i32,
    pub last_run_at: Option<DateTimeUtc>,
    pub next_run_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
#[allow(dead_code)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
