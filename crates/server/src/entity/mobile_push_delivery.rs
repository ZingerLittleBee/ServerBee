use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mobile_push_deliveries")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub device_registration_id: String,
    pub ticket_id: Option<String>,
    pub ticket_status: String,
    pub receipt_status: Option<String>,
    pub receipt_message: Option<String>,
    pub alert_key: String,
    pub alert_status: String,
    pub sent_at: DateTimeUtc,
    pub receipt_checked_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::mobile_device_registration::Entity",
        from = "Column::DeviceRegistrationId",
        to = "super::mobile_device_registration::Column::Id"
    )]
    DeviceRegistration,
}

impl Related<super::mobile_device_registration::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::DeviceRegistration.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
