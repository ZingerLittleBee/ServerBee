use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mobile_device_registrations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub user_id: String,
    #[sea_orm(unique)]
    pub installation_id: String,
    pub platform: String,
    pub push_token: Option<String>,
    pub provider: String,
    pub app_version: String,
    pub locale: String,
    pub permission_status: String,
    pub firing_alerts_push: bool,
    pub resolved_alerts_push: bool,
    pub disabled_at: Option<DateTimeUtc>,
    pub last_seen_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
