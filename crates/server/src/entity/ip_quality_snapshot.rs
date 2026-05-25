use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = IpQualitySnapshot)]
#[sea_orm(table_name = "ip_quality_snapshot")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub server_id: String,
    pub ip: String,
    pub asn: Option<String>,
    pub as_org: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub ip_type: String,
    pub is_proxy: bool,
    pub is_vpn: bool,
    pub is_hosting: bool,
    pub risk_score: Option<i32>,
    pub risk_level: String,
    pub is_tor: bool,
    pub is_abuser: bool,
    pub is_mobile: bool,
    pub asn_abuser_score: Option<i32>,
    pub abuse_email: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub checked_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
