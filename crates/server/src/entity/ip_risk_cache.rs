use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = IpRiskCache)]
#[sea_orm(table_name = "ip_risk_cache")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
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
    /// JSON: raw per-provider results
    pub providers: String,
    #[schema(value_type = String, format = DateTime)]
    pub checked_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
