// crates/server/src/entity/traceroute_record.rs
use sea_orm::entity::prelude::*;
use serverbee_common::protocol::RecordedProtocol;

#[derive(
    Clone,
    Debug,
    PartialEq,
    DeriveEntityModel,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    utoipa::ToSchema,
)]
#[sea_orm(table_name = "traceroute_record")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub server_id: String,
    pub target: String,
    /// Stored as the lowercase string form of `RecordedProtocol`. We expose
    /// it as `String` at the column level (sea-orm) and convert via
    /// `RecordedProtocol::try_from(s.as_str())` in the service layer.
    pub protocol: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub total_rounds: i32,
    pub completed_rounds: i32,
    /// Full hops Vec serialized as JSON.
    pub hops_json: String,
    pub error: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    pub fn protocol_enum(&self) -> RecordedProtocol {
        match self.protocol.as_str() {
            "icmp" => RecordedProtocol::Icmp,
            "udp"  => RecordedProtocol::Udp,
            "tcp"  => RecordedProtocol::Tcp,
            _      => RecordedProtocol::Legacy, // permissive read: covers "legacy" and any unknown value
        }
    }
}

pub fn protocol_to_str(p: RecordedProtocol) -> &'static str {
    match p {
        RecordedProtocol::Icmp => "icmp",
        RecordedProtocol::Udp  => "udp",
        RecordedProtocol::Tcp  => "tcp",
        RecordedProtocol::Legacy => "legacy",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_enum_roundtrip() {
        for p in [RecordedProtocol::Icmp, RecordedProtocol::Udp, RecordedProtocol::Tcp, RecordedProtocol::Legacy] {
            let s = protocol_to_str(p);
            let m = Model {
                id: "x".into(), server_id: "s".into(), target: "t".into(),
                protocol: s.to_string(), started_at: 0, completed_at: None,
                total_rounds: 1, completed_rounds: 1, hops_json: "[]".into(), error: None,
            };
            assert_eq!(m.protocol_enum(), p);
        }
    }

    #[test]
    fn test_unknown_protocol_string_maps_to_legacy() {
        let m = Model {
            id: "x".into(), server_id: "s".into(), target: "t".into(),
            protocol: "bogus".into(), started_at: 0, completed_at: None,
            total_rounds: 1, completed_rounds: 1, hops_json: "[]".into(), error: None,
        };
        assert_eq!(m.protocol_enum(), RecordedProtocol::Legacy);
    }
}
