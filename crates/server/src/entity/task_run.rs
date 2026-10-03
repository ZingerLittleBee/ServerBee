use sea_orm::entity::prelude::*;

/// Durable ownership and completeness of one scheduled execution.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "task_runs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub run_id: String,
    pub task_id: String,
    pub owner_id: String,
    pub manual: bool,
    pub targets_json: String,
    pub status: String,
    /// Original final-outcome time, fixed when the scheduler drains.
    pub completed_at: Option<i64>,
    /// Final counts, durable independently of outbox admission and result retention.
    pub summary_json: Option<String>,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
