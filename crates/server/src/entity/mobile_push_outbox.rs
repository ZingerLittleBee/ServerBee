use sea_orm::entity::prelude::*;

/// Delivery metadata and ciphertext only. Credentials stay in registration.
#[derive(Clone, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mobile_push_outbox")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub event_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub installation_id: String,
    pub user_id: String,
    pub mobile_session_id: String,
    pub registration_revision: i64,
    pub recipient_role: String,
    pub category: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub envelope: Option<String>,
    pub outcome: String,
    pub reason: String,
    pub attempts: i64,
    pub next_attempt_at: i64,
    pub lease_id: Option<String>,
    pub lease_until: i64,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MobilePushDelivery")
            .field("event_id", &self.event_id)
            .field("outcome", &self.outcome)
            .field("attempts", &self.attempts)
            .finish_non_exhaustive()
    }
}
