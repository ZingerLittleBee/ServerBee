use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = ServerRecordHourly)]
#[sea_orm(table_name = "records_hourly")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub server_id: String,
    #[schema(value_type = String, format = DateTime)]
    pub time: DateTimeUtc,
    pub cpu: f64,
    pub mem_used: i64,
    pub swap_used: i64,
    pub disk_used: i64,
    pub net_in_speed: i64,
    pub net_out_speed: i64,
    pub net_in_transfer: i64,
    pub net_out_transfer: i64,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub tcp_conn: i32,
    pub udp_conn: i32,
    pub process_count: i32,
    pub temperature: Option<f64>,
    pub gpu_usage: Option<f64>,
    pub disk_io_json: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// `records_hourly` deliberately mirrors the `records` column set (the rollup
/// policy manages both), so an hourly row converts losslessly into the raw-row
/// shape for consumers that don't care which resolution served them.
impl From<Model> for super::record::Model {
    fn from(m: Model) -> Self {
        super::record::Model {
            id: m.id,
            server_id: m.server_id,
            time: m.time,
            cpu: m.cpu,
            mem_used: m.mem_used,
            swap_used: m.swap_used,
            disk_used: m.disk_used,
            net_in_speed: m.net_in_speed,
            net_out_speed: m.net_out_speed,
            net_in_transfer: m.net_in_transfer,
            net_out_transfer: m.net_out_transfer,
            load1: m.load1,
            load5: m.load5,
            load15: m.load15,
            tcp_conn: m.tcp_conn,
            udp_conn: m.udp_conn,
            process_count: m.process_count,
            temperature: m.temperature,
            gpu_usage: m.gpu_usage,
            disk_io_json: m.disk_io_json,
        }
    }
}
