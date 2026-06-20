use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{Duration, Timelike, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, DatabaseConnection, EntityTrait,
    QueryFilter, Set,
};
use tokio::sync::mpsc;

use crate::config::AppConfig;
use crate::entity::{
    network_probe_config, network_probe_record, network_probe_record_hourly, network_probe_target,
    record, record_hourly, server, server_group, server_tag, traffic_daily, traffic_hourly,
    uptime_daily, user,
};
use crate::error::AppError;
use crate::service::auth::AuthService;
use crate::service::network_probe::NetworkProbeService;
use crate::service::server::ServerService;
use crate::state::AppState;
use serverbee_common::constants::{CAP_DEFAULT, PROTOCOL_VERSION};
use serverbee_common::protocol::{BrowserMessage, ServerMessage};
use serverbee_common::types::{DiskIo, NetworkProbeResultData, SystemReport};

pub const DEMO_SERVER_ID_PREFIX: &str = "dev-demo-";
pub const DEMO_SERVER_COUNT: usize = 12;

const DEMO_ADMIN_PASSWORD: &str = "admin123";
const RAW_POINTS_PER_SERVER: usize = 144;
const HOURLY_POINTS_PER_SERVER: usize = 336;
const TRAFFIC_DAYS: usize = 30;
const NETWORK_RAW_POINTS_PER_TARGET: usize = 24;
const NETWORK_HOURLY_POINTS_PER_TARGET: usize = 72;
const INSERT_BATCH_SIZE: usize = 40;
const GIB: i64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy)]
struct DemoServerSpec {
    id_suffix: &'static str,
    name: &'static str,
    group_id: &'static str,
    tags: &'static [&'static str],
    country_code: &'static str,
    region: &'static str,
    os: &'static str,
    cpu_name: &'static str,
    cpu_cores: i32,
    mem_gib: i64,
    swap_gib: i64,
    disk_gib: i64,
    ipv4: &'static str,
    price: f64,
}

#[derive(Clone, Copy)]
struct DemoGroupSpec {
    id: &'static str,
    name: &'static str,
    weight: i32,
}

#[derive(Clone, Copy)]
struct DemoTargetSpec {
    id: &'static str,
    name: &'static str,
    provider: &'static str,
    location: &'static str,
    target: &'static str,
    probe_type: &'static str,
}

const DEMO_GROUPS: [DemoGroupSpec; 3] = [
    DemoGroupSpec {
        id: "dev-demo-edge",
        name: "Demo / Edge",
        weight: 10,
    },
    DemoGroupSpec {
        id: "dev-demo-core",
        name: "Demo / Core",
        weight: 20,
    },
    DemoGroupSpec {
        id: "dev-demo-lab",
        name: "Demo / Lab",
        weight: 30,
    },
];

const DEMO_TARGETS: [DemoTargetSpec; 3] = [
    DemoTargetSpec {
        id: "dev-demo-cloudflare",
        name: "Cloudflare DNS",
        provider: "cloudflare",
        location: "global",
        target: "1.1.1.1",
        probe_type: "icmp",
    },
    DemoTargetSpec {
        id: "dev-demo-google",
        name: "Google DNS",
        provider: "google",
        location: "global",
        target: "8.8.8.8",
        probe_type: "icmp",
    },
    DemoTargetSpec {
        id: "dev-demo-tokyo",
        name: "Tokyo HTTP",
        provider: "demo",
        location: "tokyo",
        target: "https://example.com",
        probe_type: "http",
    },
];

const DEMO_SERVERS: [DemoServerSpec; DEMO_SERVER_COUNT] = [
    DemoServerSpec {
        id_suffix: "01",
        name: "demo-sfo-edge-01",
        group_id: "dev-demo-edge",
        tags: &["demo", "edge", "us"],
        country_code: "US",
        region: "California",
        os: "Ubuntu 24.04 LTS",
        cpu_name: "AMD EPYC 7B13",
        cpu_cores: 4,
        mem_gib: 8,
        swap_gib: 2,
        disk_gib: 160,
        ipv4: "203.0.113.11",
        price: 12.0,
    },
    DemoServerSpec {
        id_suffix: "02",
        name: "demo-lax-edge-02",
        group_id: "dev-demo-edge",
        tags: &["demo", "edge", "us"],
        country_code: "US",
        region: "California",
        os: "Debian 12",
        cpu_name: "Intel Xeon Platinum 8370C",
        cpu_cores: 4,
        mem_gib: 8,
        swap_gib: 2,
        disk_gib: 120,
        ipv4: "203.0.113.12",
        price: 10.5,
    },
    DemoServerSpec {
        id_suffix: "03",
        name: "demo-nrt-edge-01",
        group_id: "dev-demo-edge",
        tags: &["demo", "edge", "asia"],
        country_code: "JP",
        region: "Tokyo",
        os: "Ubuntu 22.04 LTS",
        cpu_name: "Ampere Altra",
        cpu_cores: 4,
        mem_gib: 12,
        swap_gib: 2,
        disk_gib: 200,
        ipv4: "198.51.100.21",
        price: 14.0,
    },
    DemoServerSpec {
        id_suffix: "04",
        name: "demo-sin-edge-01",
        group_id: "dev-demo-edge",
        tags: &["demo", "edge", "asia"],
        country_code: "SG",
        region: "Singapore",
        os: "Ubuntu 24.04 LTS",
        cpu_name: "AMD EPYC 7763",
        cpu_cores: 8,
        mem_gib: 16,
        swap_gib: 4,
        disk_gib: 320,
        ipv4: "198.51.100.22",
        price: 22.0,
    },
    DemoServerSpec {
        id_suffix: "05",
        name: "demo-fra-core-01",
        group_id: "dev-demo-core",
        tags: &["demo", "core", "eu"],
        country_code: "DE",
        region: "Hesse",
        os: "Debian 12",
        cpu_name: "AMD EPYC 9354P",
        cpu_cores: 8,
        mem_gib: 32,
        swap_gib: 4,
        disk_gib: 500,
        ipv4: "192.0.2.31",
        price: 32.0,
    },
    DemoServerSpec {
        id_suffix: "06",
        name: "demo-ams-core-01",
        group_id: "dev-demo-core",
        tags: &["demo", "core", "eu"],
        country_code: "NL",
        region: "North Holland",
        os: "Ubuntu 22.04 LTS",
        cpu_name: "Intel Xeon Gold 6338",
        cpu_cores: 8,
        mem_gib: 24,
        swap_gib: 4,
        disk_gib: 420,
        ipv4: "192.0.2.32",
        price: 28.0,
    },
    DemoServerSpec {
        id_suffix: "07",
        name: "demo-hkg-core-01",
        group_id: "dev-demo-core",
        tags: &["demo", "core", "asia"],
        country_code: "HK",
        region: "Hong Kong",
        os: "AlmaLinux 9",
        cpu_name: "AMD EPYC 7R13",
        cpu_cores: 6,
        mem_gib: 16,
        swap_gib: 4,
        disk_gib: 300,
        ipv4: "198.51.100.41",
        price: 26.0,
    },
    DemoServerSpec {
        id_suffix: "08",
        name: "demo-syd-core-01",
        group_id: "dev-demo-core",
        tags: &["demo", "core", "oceania"],
        country_code: "AU",
        region: "New South Wales",
        os: "Ubuntu 24.04 LTS",
        cpu_name: "Ampere Altra",
        cpu_cores: 4,
        mem_gib: 16,
        swap_gib: 4,
        disk_gib: 260,
        ipv4: "203.0.113.51",
        price: 24.0,
    },
    DemoServerSpec {
        id_suffix: "09",
        name: "demo-nyc-lab-01",
        group_id: "dev-demo-lab",
        tags: &["demo", "lab", "us"],
        country_code: "US",
        region: "New York",
        os: "Fedora 40",
        cpu_name: "Intel Xeon E-2388G",
        cpu_cores: 2,
        mem_gib: 4,
        swap_gib: 1,
        disk_gib: 80,
        ipv4: "203.0.113.61",
        price: 6.0,
    },
    DemoServerSpec {
        id_suffix: "10",
        name: "demo-tor-lab-01",
        group_id: "dev-demo-lab",
        tags: &["demo", "lab", "ca"],
        country_code: "CA",
        region: "Ontario",
        os: "Debian 12",
        cpu_name: "AMD Ryzen 7950X",
        cpu_cores: 2,
        mem_gib: 6,
        swap_gib: 1,
        disk_gib: 100,
        ipv4: "203.0.113.62",
        price: 7.5,
    },
    DemoServerSpec {
        id_suffix: "11",
        name: "demo-lon-lab-01",
        group_id: "dev-demo-lab",
        tags: &["demo", "lab", "eu"],
        country_code: "GB",
        region: "England",
        os: "Ubuntu 22.04 LTS",
        cpu_name: "Intel Xeon Silver 4314",
        cpu_cores: 2,
        mem_gib: 4,
        swap_gib: 1,
        disk_gib: 90,
        ipv4: "192.0.2.71",
        price: 8.0,
    },
    DemoServerSpec {
        id_suffix: "12",
        name: "demo-blr-lab-01",
        group_id: "dev-demo-lab",
        tags: &["demo", "lab", "asia"],
        country_code: "IN",
        region: "Karnataka",
        os: "Debian 12",
        cpu_name: "AMD EPYC 7713",
        cpu_cores: 2,
        mem_gib: 4,
        swap_gib: 1,
        disk_gib: 100,
        ipv4: "198.51.100.72",
        price: 7.0,
    },
];

pub fn validate_demo_config(config: &AppConfig) -> anyhow::Result<()> {
    if !config.dev.demo_data {
        return Ok(());
    }

    if config.database.path != "dev-demo.db" {
        anyhow::bail!(
            "dev.demo_data is destructive and only allowed with database.path = dev-demo.db"
        );
    }

    Ok(())
}

pub async fn seed_demo_data(db: &DatabaseConnection) -> Result<(), AppError> {
    let now = current_hour();

    seed_demo_admin(db).await?;
    delete_existing_demo_data(db).await?;
    seed_groups(db, now).await?;
    seed_network_targets(db, now).await?;
    seed_servers(db, now).await?;
    seed_metric_records(db, now).await?;
    seed_traffic_records(db, now).await?;
    seed_uptime_records(db, now).await?;
    seed_network_records(db, now).await?;

    tracing::info!(
        "Seeded local demo data: {} servers, {}h raw metrics, {}d hourly metrics",
        DEMO_SERVER_COUNT,
        RAW_POINTS_PER_SERVER / 6,
        HOURLY_POINTS_PER_SERVER / 24
    );

    Ok(())
}

pub async fn start_demo_agents(state: Arc<AppState>) -> Result<(), AppError> {
    let demo_servers = server::Entity::find()
        .filter(server::Column::Id.like(format!("{DEMO_SERVER_ID_PREFIX}%")))
        .all(&state.db)
        .await?;

    for (index, demo_server) in demo_servers.into_iter().enumerate() {
        let spec = DEMO_SERVERS
            .iter()
            .find(|spec| demo_server.id == demo_server_id(spec))
            .copied()
            .unwrap_or(DEMO_SERVERS[index % DEMO_SERVERS.len()]);

        let (tx, rx) = mpsc::channel::<ServerMessage>(32);
        let addr = demo_agent_addr(index);
        state.agent_manager.add_connection(
            demo_server.id.clone(),
            demo_server.name.clone(),
            tx,
            addr,
        );
        state
            .agent_manager
            .update_agent_local_capabilities(&demo_server.id, CAP_DEFAULT);
        state
            .agent_manager
            .set_protocol_version(&demo_server.id, PROTOCOL_VERSION);
        state.agent_manager.update_agent_platform(
            &demo_server.id,
            spec.os.to_string(),
            "x86_64".to_string(),
        );
        state.agent_manager.update_report(
            &demo_server.id,
            build_report(spec, live_sample_index(index), 60),
        );

        spawn_demo_agent_reporter(state.clone(), demo_server.id, spec, rx, index);
    }

    tracing::info!("Started {} in-memory demo agents", DEMO_SERVER_COUNT);
    Ok(())
}

fn spawn_demo_agent_reporter(
    state: Arc<AppState>,
    server_id: String,
    spec: DemoServerSpec,
    mut rx: mpsc::Receiver<ServerMessage>,
    index: usize,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(StdDuration::from_secs(3));
        loop {
            tokio::select! {
                maybe_msg = rx.recv() => {
                    if maybe_msg.is_none() {
                        break;
                    }
                }
                _ = interval.tick() => {
                    let sample = live_sample_index(index);
                    state.agent_manager.update_report(&server_id, build_report(spec, sample, 60));
                    let probe_results = build_network_probe_results(index, sample, Utc::now());
                    match NetworkProbeService::save_results(&state.db, &server_id, probe_results.clone()).await {
                        Ok(()) => {
                            let _ = state.browser_tx.send(BrowserMessage::NetworkProbeUpdate {
                                server_id: server_id.clone(),
                                results: probe_results,
                            });
                        }
                        Err(e) => {
                            tracing::warn!("Failed to save demo network probe result for {server_id}: {e}");
                        }
                    }
                }
            }
        }
    });
}

async fn seed_demo_admin(db: &DatabaseConnection) -> Result<(), AppError> {
    let now = Utc::now();
    let password_hash = AuthService::hash_password(DEMO_ADMIN_PASSWORD)?;
    let existing = user::Entity::find()
        .filter(user::Column::Username.eq(AuthService::DEFAULT_ADMIN_USERNAME))
        .one(db)
        .await?;

    if let Some(existing) = existing {
        let mut active: user::ActiveModel = existing.into();
        active.password_hash = Set(password_hash);
        active.role = Set("admin".to_string());
        active.must_change_password = Set(false);
        active.updated_at = Set(now);
        active.update(db).await?;
        return Ok(());
    }

    user::ActiveModel {
        id: Set("dev-demo-admin".to_string()),
        username: Set(AuthService::DEFAULT_ADMIN_USERNAME.to_string()),
        password_hash: Set(password_hash),
        role: Set("admin".to_string()),
        totp_secret: Set(None),
        must_change_password: Set(false),
        password_changed_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await?;

    Ok(())
}

async fn delete_existing_demo_data(db: &DatabaseConnection) -> Result<(), AppError> {
    let demo_ids = server::Entity::find()
        .filter(server::Column::Id.like(format!("{DEMO_SERVER_ID_PREFIX}%")))
        .all(db)
        .await?
        .into_iter()
        .map(|server| server.id)
        .collect::<Vec<_>>();

    if !demo_ids.is_empty() {
        ServerService::batch_delete(db, &demo_ids).await?;
    }

    server_group::Entity::delete_many()
        .filter(server_group::Column::Id.like("dev-demo-%"))
        .exec(db)
        .await?;
    network_probe_target::Entity::delete_many()
        .filter(network_probe_target::Column::Id.like("dev-demo-%"))
        .exec(db)
        .await?;

    Ok(())
}

async fn seed_groups(db: &DatabaseConnection, now: chrono::DateTime<Utc>) -> Result<(), AppError> {
    for group in DEMO_GROUPS {
        server_group::ActiveModel {
            id: Set(group.id.to_string()),
            name: Set(group.name.to_string()),
            weight: Set(group.weight),
            created_at: Set(now),
        }
        .insert(db)
        .await?;
    }
    Ok(())
}

async fn seed_network_targets(
    db: &DatabaseConnection,
    now: chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    for target in DEMO_TARGETS {
        network_probe_target::ActiveModel {
            id: Set(target.id.to_string()),
            name: Set(target.name.to_string()),
            provider: Set(target.provider.to_string()),
            location: Set(target.location.to_string()),
            target: Set(target.target.to_string()),
            probe_type: Set(target.probe_type.to_string()),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await?;
    }
    Ok(())
}

async fn seed_servers(db: &DatabaseConnection, now: chrono::DateTime<Utc>) -> Result<(), AppError> {
    let token_hash = AuthService::hash_password("dev-demo-agent-token")?;

    for (index, spec) in DEMO_SERVERS.iter().enumerate() {
        let id = demo_server_id(spec);
        server::ActiveModel {
            id: Set(id.clone()),
            token_hash: Set(Some(token_hash.clone())),
            token_prefix: Set(Some("dev-demo".to_string())),
            name: Set(spec.name.to_string()),
            cpu_name: Set(Some(spec.cpu_name.to_string())),
            cpu_cores: Set(Some(spec.cpu_cores)),
            cpu_arch: Set(Some("x86_64".to_string())),
            os: Set(Some(spec.os.to_string())),
            kernel_version: Set(Some("6.8.0-demo".to_string())),
            mem_total: Set(Some(spec.mem_gib * GIB)),
            swap_total: Set(Some(spec.swap_gib * GIB)),
            disk_total: Set(Some(spec.disk_gib * GIB)),
            ipv4: Set(Some(spec.ipv4.to_string())),
            ipv6: Set(None),
            region: Set(Some(spec.region.to_string())),
            country_code: Set(Some(spec.country_code.to_string())),
            virtualization: Set(Some("kvm".to_string())),
            agent_version: Set(Some(format!(
                "{}-demo",
                serverbee_common::constants::VERSION
            ))),
            group_id: Set(Some(spec.group_id.to_string())),
            weight: Set(index as i32),
            hidden: Set(false),
            remark: Set(Some("Local synthetic demo server".to_string())),
            public_remark: Set(None),
            price: Set(Some(spec.price)),
            billing_cycle: Set(Some("monthly".to_string())),
            currency: Set(Some("USD".to_string())),
            expired_at: Set(None),
            traffic_limit: Set(Some((spec.mem_gib + spec.disk_gib / 10) * 100 * GIB)),
            traffic_limit_type: Set(Some("monthly".to_string())),
            billing_start_day: Set(Some(1 + (index % 27) as i32)),
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(PROTOCOL_VERSION as i32),
            features: Set("[]".to_string()),
            last_remote_addr: Set(Some(spec.ipv4.to_string())),
            fingerprint: Set(Some(format!("dev-demo-fingerprint-{}", spec.id_suffix))),
            created_at: Set(now - Duration::days(45 - index as i64)),
            updated_at: Set(now - Duration::seconds((index % 4) as i64 * 11)),
        }
        .insert(db)
        .await?;

        for tag in spec.tags {
            server_tag::ActiveModel {
                server_id: Set(id.clone()),
                tag: Set((*tag).to_string()),
            }
            .insert(db)
            .await?;
        }

        for target in DEMO_TARGETS {
            network_probe_config::ActiveModel {
                id: Set(format!("{id}-{}", target.id)),
                server_id: Set(id.clone()),
                target_id: Set(target.id.to_string()),
                created_at: Set(now),
            }
            .insert(db)
            .await?;
        }
    }

    Ok(())
}

async fn seed_metric_records(
    db: &DatabaseConnection,
    now: chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    let mut raw_batch = Vec::with_capacity(INSERT_BATCH_SIZE);
    let mut hourly_batch = Vec::with_capacity(INSERT_BATCH_SIZE);

    for spec in DEMO_SERVERS {
        let server_id = demo_server_id(&spec);

        for point in 0..RAW_POINTS_PER_SERVER {
            let age = (RAW_POINTS_PER_SERVER - point) as i64 * 10;
            let time = now - Duration::minutes(age);
            let report = build_report(spec, point, 600);
            raw_batch.push(record_model(&server_id, time, report)?);
            if raw_batch.len() >= INSERT_BATCH_SIZE {
                flush_record_batch(db, &mut raw_batch).await?;
            }
        }

        for point in 0..HOURLY_POINTS_PER_SERVER {
            let age = (HOURLY_POINTS_PER_SERVER - point) as i64;
            let time = now - Duration::hours(age);
            let report = build_report(spec, point, 3600);
            hourly_batch.push(record_hourly_model(&server_id, time, report)?);
            if hourly_batch.len() >= INSERT_BATCH_SIZE {
                flush_record_hourly_batch(db, &mut hourly_batch).await?;
            }
        }
    }

    flush_record_batch(db, &mut raw_batch).await?;
    flush_record_hourly_batch(db, &mut hourly_batch).await?;
    Ok(())
}

async fn seed_traffic_records(
    db: &DatabaseConnection,
    now: chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    let mut hourly_batch = Vec::with_capacity(INSERT_BATCH_SIZE);
    let mut daily_batch = Vec::with_capacity(INSERT_BATCH_SIZE);

    for spec in DEMO_SERVERS {
        let server_id = demo_server_id(&spec);
        for hour_index in 0..(7 * 24) {
            let age = (7 * 24 - hour_index) as i64;
            hourly_batch.push(traffic_hourly::ActiveModel {
                id: NotSet,
                server_id: Set(server_id.clone()),
                hour: Set(now - Duration::hours(age)),
                bytes_in: Set(traffic_bytes(spec, hour_index, 23)),
                bytes_out: Set(traffic_bytes(spec, hour_index, 17)),
            });
            if hourly_batch.len() >= INSERT_BATCH_SIZE {
                flush_traffic_hourly_batch(db, &mut hourly_batch).await?;
            }
        }

        for day_index in 0..TRAFFIC_DAYS {
            let age = (TRAFFIC_DAYS - day_index) as i64;
            daily_batch.push(traffic_daily::ActiveModel {
                id: NotSet,
                server_id: Set(server_id.clone()),
                date: Set((now - Duration::days(age)).date_naive()),
                bytes_in: Set(traffic_bytes(spec, day_index, 251) * 12),
                bytes_out: Set(traffic_bytes(spec, day_index, 193) * 9),
            });
            if daily_batch.len() >= INSERT_BATCH_SIZE {
                flush_traffic_daily_batch(db, &mut daily_batch).await?;
            }
        }
    }

    flush_traffic_hourly_batch(db, &mut hourly_batch).await?;
    flush_traffic_daily_batch(db, &mut daily_batch).await?;
    Ok(())
}

async fn seed_uptime_records(
    db: &DatabaseConnection,
    now: chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    let mut batch = Vec::with_capacity(INSERT_BATCH_SIZE);

    for (server_index, spec) in DEMO_SERVERS.iter().enumerate() {
        let server_id = demo_server_id(spec);
        for day_index in 0..TRAFFIC_DAYS {
            let total = 1440;
            let wobble = ((server_index * 13 + day_index * 7) % 19) as i32;
            let incident = i32::from((server_index + day_index).is_multiple_of(17));
            batch.push(uptime_daily::ActiveModel {
                id: NotSet,
                server_id: Set(server_id.clone()),
                date: Set((now - Duration::days((TRAFFIC_DAYS - day_index) as i64)).date_naive()),
                total_minutes: Set(total),
                online_minutes: Set(total - wobble - incident * 45),
                downtime_incidents: Set(incident),
            });
            if batch.len() >= INSERT_BATCH_SIZE {
                flush_uptime_batch(db, &mut batch).await?;
            }
        }
    }

    flush_uptime_batch(db, &mut batch).await?;
    Ok(())
}

async fn seed_network_records(
    db: &DatabaseConnection,
    now: chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    let mut raw_batch = Vec::with_capacity(INSERT_BATCH_SIZE);
    let mut hourly_batch = Vec::with_capacity(INSERT_BATCH_SIZE);
    let raw_now = Utc::now();

    for (server_index, spec) in DEMO_SERVERS.iter().enumerate() {
        let server_id = demo_server_id(spec);
        for (target_index, target) in DEMO_TARGETS.iter().enumerate() {
            for point in 0..NETWORK_RAW_POINTS_PER_TARGET {
                let latency = latency_ms(server_index, target_index, point);
                let loss = packet_loss(server_index, target_index, point);
                raw_batch.push(network_probe_record::ActiveModel {
                    id: NotSet,
                    server_id: Set(server_id.clone()),
                    target_id: Set(target.id.to_string()),
                    avg_latency: Set(Some(latency)),
                    min_latency: Set(Some((latency * 0.72).max(1.0))),
                    max_latency: Set(Some(latency * 1.35)),
                    packet_loss: Set(loss),
                    packet_sent: Set(10),
                    packet_received: Set(packet_received_from_loss(loss) as i32),
                    timestamp: Set(raw_now
                        - Duration::minutes(
                            (NETWORK_RAW_POINTS_PER_TARGET - 1 - point) as i64 * 3,
                        )),
                });
                if raw_batch.len() >= INSERT_BATCH_SIZE {
                    flush_network_raw_batch(db, &mut raw_batch).await?;
                }
            }

            for point in 0..NETWORK_HOURLY_POINTS_PER_TARGET {
                let latency = latency_ms(server_index, target_index, point);
                hourly_batch.push(network_probe_record_hourly::ActiveModel {
                    id: NotSet,
                    server_id: Set(server_id.clone()),
                    target_id: Set(target.id.to_string()),
                    avg_latency: Set(Some(latency)),
                    min_latency: Set(Some((latency * 0.74).max(1.0))),
                    max_latency: Set(Some(latency * 1.42)),
                    avg_packet_loss: Set(packet_loss(server_index, target_index, point)),
                    sample_count: Set(4),
                    hour: Set(
                        now - Duration::hours((NETWORK_HOURLY_POINTS_PER_TARGET - point) as i64)
                    ),
                });
                if hourly_batch.len() >= INSERT_BATCH_SIZE {
                    flush_network_hourly_batch(db, &mut hourly_batch).await?;
                }
            }
        }
    }

    flush_network_raw_batch(db, &mut raw_batch).await?;
    flush_network_hourly_batch(db, &mut hourly_batch).await?;
    Ok(())
}

fn build_network_probe_results(
    server_index: usize,
    sample: usize,
    timestamp: chrono::DateTime<Utc>,
) -> Vec<NetworkProbeResultData> {
    DEMO_TARGETS
        .iter()
        .enumerate()
        .map(|(target_index, target)| {
            let latency = latency_ms(server_index, target_index, sample);
            let loss = packet_loss(server_index, target_index, sample);
            NetworkProbeResultData {
                target_id: target.id.to_string(),
                avg_latency: Some(latency),
                min_latency: Some((latency * 0.72).max(1.0)),
                max_latency: Some(latency * 1.35),
                packet_loss: loss,
                packet_sent: 10,
                packet_received: packet_received_from_loss(loss),
                timestamp,
            }
        })
        .collect()
}

fn record_model(
    server_id: &str,
    time: chrono::DateTime<Utc>,
    report: SystemReport,
) -> Result<record::ActiveModel, AppError> {
    Ok(record::ActiveModel {
        id: NotSet,
        server_id: Set(server_id.to_string()),
        time: Set(time),
        cpu: Set(report.cpu),
        mem_used: Set(report.mem_used),
        swap_used: Set(report.swap_used),
        disk_used: Set(report.disk_used),
        net_in_speed: Set(report.net_in_speed),
        net_out_speed: Set(report.net_out_speed),
        net_in_transfer: Set(report.net_in_transfer),
        net_out_transfer: Set(report.net_out_transfer),
        load1: Set(report.load1),
        load5: Set(report.load5),
        load15: Set(report.load15),
        tcp_conn: Set(report.tcp_conn),
        udp_conn: Set(report.udp_conn),
        process_count: Set(report.process_count),
        temperature: Set(report.temperature),
        gpu_usage: Set(report.gpu.as_ref().map(|gpu| gpu.average_usage)),
        disk_io_json: Set(serialize_disk_io(&report)?),
    })
}

fn record_hourly_model(
    server_id: &str,
    time: chrono::DateTime<Utc>,
    report: SystemReport,
) -> Result<record_hourly::ActiveModel, AppError> {
    Ok(record_hourly::ActiveModel {
        id: NotSet,
        server_id: Set(server_id.to_string()),
        time: Set(time),
        cpu: Set(report.cpu),
        mem_used: Set(report.mem_used),
        swap_used: Set(report.swap_used),
        disk_used: Set(report.disk_used),
        net_in_speed: Set(report.net_in_speed),
        net_out_speed: Set(report.net_out_speed),
        net_in_transfer: Set(report.net_in_transfer),
        net_out_transfer: Set(report.net_out_transfer),
        load1: Set(report.load1),
        load5: Set(report.load5),
        load15: Set(report.load15),
        tcp_conn: Set(report.tcp_conn),
        udp_conn: Set(report.udp_conn),
        process_count: Set(report.process_count),
        temperature: Set(report.temperature),
        gpu_usage: Set(report.gpu.as_ref().map(|gpu| gpu.average_usage)),
        disk_io_json: Set(serialize_disk_io(&report)?),
    })
}

fn serialize_disk_io(report: &SystemReport) -> Result<Option<String>, AppError> {
    report
        .disk_io
        .as_ref()
        .map(|entries| {
            serde_json::to_string(entries)
                .map_err(|e| AppError::Internal(format!("Disk I/O serialization error: {e}")))
        })
        .transpose()
}

fn build_report(spec: DemoServerSpec, sample: usize, interval_secs: i64) -> SystemReport {
    let seed = spec.id_suffix.parse::<usize>().unwrap_or(1);
    let mem_total = spec.mem_gib * GIB;
    let swap_total = spec.swap_gib * GIB;
    let disk_total = spec.disk_gib * GIB;
    let cpu = 8.0 + wave(seed, sample, 71) as f64 + (seed % 7) as f64 * 0.35;
    let mem_pct = 38 + wave(seed + 5, sample, 47) as i64;
    let disk_pct = 22 + ((seed * 3 + sample / 37) % 58) as i64;
    let net_in_speed = 40_000 + wave(seed + 11, sample, 840) as i64 * 1400;
    let net_out_speed = 18_000 + wave(seed + 13, sample, 560) as i64 * 1100;
    let transfer_base = seed as i64 * 400 * GIB;
    let transfer_step = sample as i64 * interval_secs.max(1);

    SystemReport {
        cpu: cpu.min(96.0),
        mem_used: mem_total * mem_pct / 100,
        swap_used: swap_total * (wave(seed + 3, sample, 35) as i64) / 100,
        disk_used: disk_total * disk_pct / 100,
        net_in_speed,
        net_out_speed,
        net_in_transfer: transfer_base + transfer_step * net_in_speed / 2,
        net_out_transfer: transfer_base / 2 + transfer_step * net_out_speed / 2,
        load1: cpu / 100.0 * spec.cpu_cores as f64,
        load5: cpu / 120.0 * spec.cpu_cores as f64,
        load15: cpu / 145.0 * spec.cpu_cores as f64,
        tcp_conn: 40 + wave(seed + 17, sample, 180) as i32,
        udp_conn: 4 + wave(seed + 19, sample, 36) as i32,
        process_count: 95 + spec.cpu_cores * 9 + wave(seed + 23, sample, 80) as i32,
        uptime: (Duration::days(7 + seed as i64).num_seconds() + sample as i64 * interval_secs)
            as u64,
        disk_io: Some(vec![
            DiskIo {
                name: "vda".to_string(),
                read_bytes_per_sec: 16_384 + wave(seed + 29, sample, 900) as u64 * 1024,
                write_bytes_per_sec: 8192 + wave(seed + 31, sample, 700) as u64 * 1024,
            },
            DiskIo {
                name: "vdb".to_string(),
                read_bytes_per_sec: 4096 + wave(seed + 37, sample, 320) as u64 * 1024,
                write_bytes_per_sec: 2048 + wave(seed + 41, sample, 260) as u64 * 1024,
            },
        ]),
        temperature: Some(38.0 + wave(seed + 43, sample, 34) as f64 * 0.55),
        gpu: None,
    }
}

fn wave(seed: usize, sample: usize, modulo: usize) -> usize {
    if modulo == 0 {
        return 0;
    }
    (seed * 31 + sample * 17 + (sample / 9) * 13) % modulo
}

fn latency_ms(server_index: usize, target_index: usize, point: usize) -> f64 {
    let base = 18.0 + target_index as f64 * 22.0 + server_index as f64 * 4.5;
    base + wave(server_index + target_index * 3, point, 55) as f64 * 0.7
}

fn packet_loss(server_index: usize, target_index: usize, point: usize) -> f64 {
    if (server_index + target_index + point).is_multiple_of(29) {
        0.2
    } else if (server_index * 3 + target_index + point).is_multiple_of(17) {
        0.1
    } else {
        0.0
    }
}

fn packet_received_from_loss(loss: f64) -> u32 {
    (10.0 * (1.0 - loss)).round().clamp(0.0, 10.0) as u32
}

fn traffic_bytes(spec: DemoServerSpec, point: usize, multiplier: usize) -> i64 {
    let seed = spec.id_suffix.parse::<usize>().unwrap_or(1);
    (20 + wave(seed + multiplier, point, 900) as i64) * 1024 * 1024
}

fn live_sample_index(offset: usize) -> usize {
    ((Utc::now().timestamp() / 3).max(0) as usize).saturating_add(offset * 11)
}

fn current_hour() -> chrono::DateTime<Utc> {
    let now = Utc::now();
    now.date_naive()
        .and_hms_opt(now.time().hour(), 0, 0)
        .expect("valid hour")
        .and_utc()
}

fn demo_server_id(spec: &DemoServerSpec) -> String {
    format!("{DEMO_SERVER_ID_PREFIX}{}", spec.id_suffix)
}

fn demo_agent_addr(index: usize) -> SocketAddr {
    SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        39_000 + index as u16,
    )
}

macro_rules! flush_batch {
    ($fn_name:ident, $entity:path, $model:path) => {
        async fn $fn_name(
            db: &DatabaseConnection,
            batch: &mut Vec<$model>,
        ) -> Result<(), AppError> {
            if batch.is_empty() {
                return Ok(());
            }
            let rows = std::mem::take(batch);
            <$entity>::insert_many(rows).exec(db).await?;
            Ok(())
        }
    };
}

flush_batch!(flush_record_batch, record::Entity, record::ActiveModel);
flush_batch!(
    flush_record_hourly_batch,
    record_hourly::Entity,
    record_hourly::ActiveModel
);
flush_batch!(
    flush_traffic_hourly_batch,
    traffic_hourly::Entity,
    traffic_hourly::ActiveModel
);
flush_batch!(
    flush_traffic_daily_batch,
    traffic_daily::Entity,
    traffic_daily::ActiveModel
);
flush_batch!(
    flush_uptime_batch,
    uptime_daily::Entity,
    uptime_daily::ActiveModel
);
flush_batch!(
    flush_network_raw_batch,
    network_probe_record::Entity,
    network_probe_record::ActiveModel
);
flush_batch!(
    flush_network_hourly_batch,
    network_probe_record_hourly::Entity,
    network_probe_record_hourly::ActiveModel
);

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

    use crate::config::AppConfig;
    use crate::dev_demo::{
        DEMO_SERVER_COUNT, DEMO_SERVER_ID_PREFIX, seed_demo_data, start_demo_agents,
    };
    use crate::entity::{record, record_hourly, server, user};
    use crate::service::auth::AuthService;
    use crate::service::network_probe::NetworkProbeService;
    use crate::state::AppState;
    use crate::test_utils::setup_test_db;

    #[tokio::test]
    async fn seed_demo_data_populates_fixed_local_dataset_idempotently() {
        let (db, _tmp) = setup_test_db().await;

        seed_demo_data(&db).await.expect("demo seed should succeed");
        seed_demo_data(&db)
            .await
            .expect("demo seed should be idempotent");

        let demo_servers = server::Entity::find()
            .filter(server::Column::Id.like(format!("{DEMO_SERVER_ID_PREFIX}%")))
            .all(&db)
            .await
            .expect("demo servers query should succeed");
        assert_eq!(demo_servers.len(), DEMO_SERVER_COUNT);
        assert!(
            demo_servers
                .iter()
                .all(|server| server.token_hash.is_some())
        );
        assert!(demo_servers.iter().any(|server| server.region.is_some()));

        let demo_ids = demo_servers
            .iter()
            .map(|server| server.id.clone())
            .collect::<Vec<_>>();
        let raw_count = record::Entity::find()
            .filter(record::Column::ServerId.is_in(demo_ids.clone()))
            .count(&db)
            .await
            .expect("raw records count should succeed");
        let hourly_count = record_hourly::Entity::find()
            .filter(record_hourly::Column::ServerId.is_in(demo_ids))
            .count(&db)
            .await
            .expect("hourly records count should succeed");
        assert!(raw_count > DEMO_SERVER_COUNT as u64);
        assert!(hourly_count > DEMO_SERVER_COUNT as u64);

        let admin = user::Entity::find()
            .filter(user::Column::Username.eq(AuthService::DEFAULT_ADMIN_USERNAME))
            .one(&db)
            .await
            .expect("admin query should succeed")
            .expect("demo seed should create admin");
        assert!(!admin.must_change_password);
        assert!(AuthService::verify_password("admin123", &admin.password_hash).unwrap());
    }

    #[tokio::test]
    async fn seed_demo_data_populates_network_records_for_immediate_one_hour_chart() {
        let (db, _tmp) = setup_test_db().await;
        let seed_started_at = Utc::now();

        seed_demo_data(&db).await.expect("demo seed should succeed");

        let recent_records = NetworkProbeService::query_records(
            &db,
            "dev-demo-01",
            None,
            seed_started_at - Duration::minutes(1),
            Utc::now() + Duration::minutes(1),
        )
        .await
        .expect("recent network probe records query should succeed");
        assert!(
            !recent_records.is_empty(),
            "demo network chart should have records immediately after startup"
        );

        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .expect("app state should build");
        let summary = NetworkProbeService::get_server_summary(
            &db,
            &state.agent_manager,
            "dev-demo-01",
            &crate::config::NetworkProbeConfig::default(),
        )
        .await
        .expect("network summary should succeed");
        let last_probe_at = summary
            .last_probe_at
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .expect("demo summary should include last probe timestamp")
            .with_timezone(&Utc);
        assert!(
            last_probe_at >= seed_started_at - Duration::minutes(1),
            "demo last probe should be recent enough for realtime and 1h views"
        );
    }

    #[tokio::test]
    async fn start_demo_agents_marks_seeded_servers_online_with_reports() {
        let (db, _tmp) = setup_test_db().await;
        seed_demo_data(&db).await.expect("demo seed should succeed");
        let state = AppState::new(db, AppConfig::default())
            .await
            .expect("app state should build");

        start_demo_agents(state.clone())
            .await
            .expect("demo agents should start");

        assert_eq!(state.agent_manager.online_count(), DEMO_SERVER_COUNT);
        let report = state
            .agent_manager
            .get_latest_report("dev-demo-01")
            .expect("first demo server should have a live report");
        assert!(report.cpu > 0.0);
        assert!(report.mem_used > 0);
    }
}
