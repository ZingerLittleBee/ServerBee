use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize)]
#[sea_orm(table_name = "servers")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub name: String,
    pub cpu_name: Option<String>,
    pub cpu_cores: Option<i32>,
    pub cpu_arch: Option<String>,
    pub os: Option<String>,
    pub kernel_version: Option<String>,
    pub mem_total: Option<i64>,
    pub swap_total: Option<i64>,
    pub disk_total: Option<i64>,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub region: Option<String>,
    pub country_code: Option<String>,
    pub virtualization: Option<String>,
    pub agent_version: Option<String>,
    #[sea_orm(indexed)]
    pub group_id: Option<String>,
    pub weight: i32,
    pub hidden: bool,
    pub remark: Option<String>,
    pub public_remark: Option<String>,
    pub price: Option<f64>,
    pub billing_cycle: Option<String>,
    pub currency: Option<String>,
    pub expired_at: Option<DateTimeUtc>,
    pub traffic_limit: Option<i64>,
    pub traffic_limit_type: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::server_group::Entity",
        from = "Column::GroupId",
        to = "super::server_group::Column::Id"
    )]
    ServerGroup,
}

impl Related<super::server_group::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ServerGroup.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
