use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "enrollment_offers")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub code_hash: String,
    #[sea_orm(indexed)]
    pub code_prefix: String,
    pub target_server_id: String,
    pub created_by: String,
    pub expires_at: DateTimeUtc,
    pub outcome: Option<String>,
    pub terminal_at: Option<DateTimeUtc>,
    pub successor_offer_id: Option<String>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::server::Entity",
        from = "Column::TargetServerId",
        to = "super::server::Column::Id"
    )]
    Server,
}

impl Related<super::server::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Server.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
