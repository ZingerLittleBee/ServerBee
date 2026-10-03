use sea_orm::entity::prelude::*;

/// A consumed refresh secret grants deletion of its original session only.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mobile_session_revocation_proofs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub mobile_session_id: String,
    pub token_hash: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::mobile_session::Entity",
        from = "Column::MobileSessionId",
        to = "super::mobile_session::Column::Id"
    )]
    MobileSession,
}

impl Related<super::mobile_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::MobileSession.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
