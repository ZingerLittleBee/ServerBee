use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "recovery_job")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub job_id: String,
    pub target_server_id: String,
    pub source_server_id: String,
    pub status: String,
    pub stage: String,
    pub checkpoint_json: Option<String>,
    pub error: Option<String>,
    pub started_at: DateTimeUtc,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub last_heartbeat_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
