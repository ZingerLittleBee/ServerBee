use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "agent_authority_events")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub server_id: String,
    pub server_name: String,
    pub actor_kind: String,
    pub actor_id: Option<String>,
    pub request_source: String,
    pub offer_id: Option<String>,
    pub transition: String,
    pub mode: Option<String>,
    pub offer_outcome: Option<String>,
    pub authority_before: String,
    pub authority_after: String,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
