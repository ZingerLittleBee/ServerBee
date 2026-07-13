use sea_orm_migration::prelude::*;

mod m20260312_000001_init;
mod m20260312_000002_oauth;
mod m20260314_000003_add_capabilities;
mod m20260315_000004_network_probe;
mod m20260317_000005_traffic_and_scheduled_tasks;
mod m20260318_000006_docker_support;
mod m20260319_000007_service_monitor;
mod m20260319_000008_disk_io_records;
mod m20260320_000009_status_page;
mod m20260320_000010_dashboard;
mod m20260321_000011_status_page_uptime_thresholds;
mod m20260327_000012_records_hourly_unique;
mod m20260329_000013_add_server_fingerprint;
mod m20260329_000014_create_mobile_session;
mod m20260329_000015_add_session_source;
mod m20260329_000016_create_device_token;
mod m20260416_000017_create_recovery_job;
mod m20260416_000018_migrate_email_to_resend;
mod m20260430_000019_create_custom_theme;
mod m20260430_000020_add_status_page_theme_ref;
mod m20260430_000021_custom_theme_ref_integrity;
mod m20260517_000022_create_agent_enrollment;
mod m20260517_000023_add_must_change_password;
mod m20260521_000024_create_security_event;
mod m20260521_000025_extend_alert_state_event_key;
mod m20260521_000026_backfill_capability_default;
mod m20260521_000027_create_block_list;
mod m20260521_000028_extend_alert_rule_actions;
mod m20260522_000029_ip_quality;
mod m20260522_000030_status_page_show_ip_quality;
mod m20260523_000031_default_caps_add_firewall_ip_quality;
mod m20260524_000032_create_traceroute_record;
mod m20260525_000033_ip_quality_snapshot_extra_fields;
mod m20260525_000034_agent_registration_redesign;
mod m20260526_000035_create_spa_themes;
mod m20260526_000036_simplify_status_page;
mod m20260528_000037_create_widget_module;
mod m20260528_000060_drop_legacy_theme_tables;
mod m20260528_000070_dashboard_widget_module_id;
mod m20260619_000071_add_password_changed_at;
mod m20260621_000072_add_geo_manual;
mod m20260702_000073_retention_time_indexes;
mod m20260702_000074_hash_existing_session_tokens;
mod m20260713_000075_agent_authority_lifecycle;

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260312_000001_init::Migration),
            Box::new(m20260312_000002_oauth::Migration),
            Box::new(m20260314_000003_add_capabilities::Migration),
            Box::new(m20260315_000004_network_probe::Migration),
            Box::new(m20260317_000005_traffic_and_scheduled_tasks::Migration),
            Box::new(m20260318_000006_docker_support::Migration),
            Box::new(m20260319_000007_service_monitor::Migration),
            Box::new(m20260319_000008_disk_io_records::Migration),
            Box::new(m20260320_000009_status_page::Migration),
            Box::new(m20260320_000010_dashboard::Migration),
            Box::new(m20260321_000011_status_page_uptime_thresholds::Migration),
            Box::new(m20260327_000012_records_hourly_unique::Migration),
            Box::new(m20260329_000013_add_server_fingerprint::Migration),
            Box::new(m20260329_000014_create_mobile_session::Migration),
            Box::new(m20260329_000015_add_session_source::Migration),
            Box::new(m20260329_000016_create_device_token::Migration),
            Box::new(m20260416_000017_create_recovery_job::Migration),
            Box::new(m20260416_000018_migrate_email_to_resend::Migration),
            Box::new(m20260430_000019_create_custom_theme::Migration),
            Box::new(m20260430_000020_add_status_page_theme_ref::Migration),
            Box::new(m20260430_000021_custom_theme_ref_integrity::Migration),
            Box::new(m20260517_000022_create_agent_enrollment::Migration),
            Box::new(m20260517_000023_add_must_change_password::Migration),
            Box::new(m20260521_000024_create_security_event::Migration),
            Box::new(m20260521_000025_extend_alert_state_event_key::Migration),
            Box::new(m20260521_000026_backfill_capability_default::Migration),
            Box::new(m20260521_000027_create_block_list::Migration),
            Box::new(m20260521_000028_extend_alert_rule_actions::Migration),
            Box::new(m20260522_000029_ip_quality::Migration),
            Box::new(m20260522_000030_status_page_show_ip_quality::Migration),
            Box::new(m20260523_000031_default_caps_add_firewall_ip_quality::Migration),
            Box::new(m20260524_000032_create_traceroute_record::Migration),
            Box::new(m20260525_000033_ip_quality_snapshot_extra_fields::Migration),
            Box::new(m20260525_000034_agent_registration_redesign::Migration),
            Box::new(m20260526_000035_create_spa_themes::Migration),
            Box::new(m20260526_000036_simplify_status_page::Migration),
            Box::new(m20260528_000037_create_widget_module::Migration),
            Box::new(m20260528_000060_drop_legacy_theme_tables::Migration),
            Box::new(m20260528_000070_dashboard_widget_module_id::Migration),
            Box::new(m20260619_000071_add_password_changed_at::Migration),
            Box::new(m20260621_000072_add_geo_manual::Migration),
            Box::new(m20260702_000073_retention_time_indexes::Migration),
            Box::new(m20260702_000074_hash_existing_session_tokens::Migration),
            Box::new(m20260713_000075_agent_authority_lifecycle::Migration),
        ]
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, DatabaseBackend, Statement};
    use sea_orm_migration::MigratorTrait;

    use super::Migrator;

    async fn migrated_db() -> sea_orm::DatabaseConnection {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        Migrator::up(&db, None).await.expect("run migrations");
        db
    }

    async fn seed_user_and_server(db: &sea_orm::DatabaseConnection, server_id: &str) {
        db.execute(Statement::from_string(
            DatabaseBackend::Sqlite,
            "INSERT OR IGNORE INTO users (id, username, password_hash, role, must_change_password, created_at, updated_at) VALUES ('actor-1', 'actor-1', 'hash', 'admin', 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)".to_string(),
        ))
        .await
        .expect("seed actor");
        db.execute(Statement::from_string(
            DatabaseBackend::Sqlite,
            format!(
                "INSERT INTO servers (id, name, weight, hidden, capabilities, protocol_version, features, geo_manual, created_at, updated_at) VALUES ('{server_id}', 'Server {server_id}', 0, 0, 0, 1, '[]', 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"
            ),
        ))
        .await
        .expect("seed server");
    }

    #[tokio::test]
    async fn agent_authority_schema_replaces_fingerprint_and_legacy_enrollments() {
        let db = migrated_db().await;

        let server_columns = db
            .query_all(Statement::from_string(
                DatabaseBackend::Sqlite,
                "PRAGMA table_info('servers')".to_string(),
            ))
            .await
            .expect("inspect servers schema");
        let server_column_names: Vec<String> = server_columns
            .iter()
            .map(|row| row.try_get("", "name").expect("column name"))
            .collect();
        assert!(
            !server_column_names.iter().any(|name| name == "fingerprint"),
            "live servers schema must not retain machine fingerprints"
        );

        let offer_columns = db
            .query_all(Statement::from_string(
                DatabaseBackend::Sqlite,
                "PRAGMA table_info('enrollment_offers')".to_string(),
            ))
            .await
            .expect("inspect enrollment offer schema");
        assert!(!offer_columns.is_empty(), "enrollment_offers must exist");

        let legacy_table = db
            .query_one(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'agent_enrollments'"
                    .to_string(),
            ))
            .await
            .expect("inspect legacy enrollment table");
        assert!(
            legacy_table.is_none(),
            "legacy agent_enrollments must be removed"
        );
    }

    #[tokio::test]
    async fn agent_authority_offer_constraints_enforce_one_exclusive_outcome() {
        let db = migrated_db().await;
        seed_user_and_server(&db, "server-1").await;

        let insert_outstanding = |id: &str| {
            Statement::from_string(
                DatabaseBackend::Sqlite,
                format!(
                    "INSERT INTO enrollment_offers (id, code_hash, code_prefix, target_server_id, created_by, expires_at, created_at) VALUES ('{id}', 'hash', 'prefix01', 'server-1', 'actor-1', '2999-01-01 00:00:00', CURRENT_TIMESTAMP)"
                ),
            )
        };
        db.execute(insert_outstanding("offer-1"))
            .await
            .expect("insert first outstanding offer");
        assert!(
            db.execute(insert_outstanding("offer-2")).await.is_err(),
            "a Server cannot have two Outstanding offers"
        );

        let missing_terminal_time = db
            .execute(Statement::from_string(
                DatabaseBackend::Sqlite,
                "INSERT INTO enrollment_offers (id, code_hash, code_prefix, target_server_id, created_by, expires_at, outcome, created_at) VALUES ('bad-consumed', 'hash', 'prefix02', 'server-1', 'actor-1', '2999-01-01 00:00:00', 'consumed', CURRENT_TIMESTAMP)".to_string(),
            ))
            .await;
        assert!(
            missing_terminal_time.is_err(),
            "a terminal outcome must carry terminal_at"
        );

        let replaced_without_successor = db
            .execute(Statement::from_string(
                DatabaseBackend::Sqlite,
                "INSERT INTO enrollment_offers (id, code_hash, code_prefix, target_server_id, created_by, expires_at, outcome, terminal_at, created_at) VALUES ('bad-replaced', 'hash', 'prefix03', 'server-1', 'actor-1', '2999-01-01 00:00:00', 'replaced', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)".to_string(),
            ))
            .await;
        assert!(
            replaced_without_successor.is_err(),
            "Replaced must identify its successor"
        );

        let successor_on_consumed = db
            .execute(Statement::from_string(
                DatabaseBackend::Sqlite,
                "INSERT INTO enrollment_offers (id, code_hash, code_prefix, target_server_id, created_by, expires_at, outcome, terminal_at, successor_offer_id, created_at) VALUES ('bad-successor', 'hash', 'prefix04', 'server-1', 'actor-1', '2999-01-01 00:00:00', 'consumed', CURRENT_TIMESTAMP, 'offer-9', CURRENT_TIMESTAMP)".to_string(),
            ))
            .await;
        assert!(
            successor_on_consumed.is_err(),
            "only Replaced may identify a successor"
        );
    }

    #[tokio::test]
    async fn agent_authority_migration_converts_legacy_terminal_facts() {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        let migrations_before_authority = Migrator::migrations().len() as u32 - 1;
        Migrator::up(&db, Some(migrations_before_authority))
            .await
            .expect("run legacy migrations");
        seed_user_and_server(&db, "legacy-server").await;
        seed_user_and_server(&db, "legacy-expired-server").await;

        db.execute(Statement::from_string(
            DatabaseBackend::Sqlite,
            r#"
            INSERT INTO agent_enrollments (id, code_hash, code_prefix, target_server_id, created_by, expires_at, consumed_at, revoked_at, created_at) VALUES
                ('double-terminal', 'hash', 'double01', 'legacy-server', 'actor-1', '2999-01-01 00:00:00', '2026-01-01 00:00:00', '2026-01-02 00:00:00', '2026-01-01 00:00:00'),
                ('revoked', 'hash', 'revoke01', 'legacy-server', 'actor-1', '2999-01-01 00:00:00', NULL, '2026-01-02 00:00:00', '2026-01-01 00:00:00'),
                ('expired', 'hash', 'expire01', 'legacy-expired-server', 'actor-1', '2026-01-01 00:00:00', NULL, NULL, '2026-01-01 00:00:00'),
                ('outstanding', 'hash', 'open0001', 'legacy-server', 'actor-1', '2999-01-01 00:00:00', NULL, NULL, '2026-01-01 00:00:00')
            "#
            .to_string(),
        ))
        .await
        .expect("seed legacy offers");

        Migrator::up(&db, None)
            .await
            .expect("run authority migration");

        let rows = db
            .query_all(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT id, outcome FROM enrollment_offers ORDER BY id".to_string(),
            ))
            .await
            .expect("read migrated offers");
        let outcomes: std::collections::HashMap<String, Option<String>> = rows
            .iter()
            .map(|row| {
                (
                    row.try_get("", "id").expect("offer id"),
                    row.try_get("", "outcome").expect("offer outcome"),
                )
            })
            .collect();
        assert_eq!(
            outcomes.get("double-terminal"),
            Some(&Some("consumed".to_string())),
            "legacy consume wins over the later revoke bug"
        );
        assert_eq!(outcomes.get("revoked"), Some(&Some("revoked".to_string())));
        assert_eq!(outcomes.get("expired"), Some(&Some("expired".to_string())));
        assert_eq!(outcomes.get("outstanding"), Some(&None));
    }

    #[tokio::test]
    async fn agent_authority_events_survive_server_deletion_without_secrets() {
        let db = migrated_db().await;
        seed_user_and_server(&db, "server-delete").await;
        db.execute(Statement::from_string(
            DatabaseBackend::Sqlite,
            "INSERT INTO enrollment_offers (id, code_hash, code_prefix, target_server_id, created_by, expires_at, created_at) VALUES ('offer-delete', 'secret-hash', 'prefix05', 'server-delete', 'actor-1', '2999-01-01 00:00:00', CURRENT_TIMESTAMP)".to_string(),
        ))
        .await
        .expect("insert offer");
        db.execute(Statement::from_string(
            DatabaseBackend::Sqlite,
            "INSERT INTO agent_authority_events (id, server_id, server_name, actor_kind, actor_id, request_source, offer_id, transition, authority_before, authority_after, created_at) VALUES ('event-delete', 'server-delete', 'Deleted Server', 'user', 'actor-1', 'api', 'offer-delete', 'server_deleted', 'unclaimed', 'unclaimed', CURRENT_TIMESTAMP)".to_string(),
        ))
        .await
        .expect("insert authority event");

        db.execute(Statement::from_string(
            DatabaseBackend::Sqlite,
            "DELETE FROM servers WHERE id = 'server-delete'".to_string(),
        ))
        .await
        .expect("delete server");

        let offer_count: i64 = db
            .query_one(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM enrollment_offers WHERE target_server_id = 'server-delete'"
                    .to_string(),
            ))
            .await
            .expect("count offers")
            .expect("count row")
            .try_get("", "count")
            .expect("offer count");
        let event_count: i64 = db
            .query_one(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM agent_authority_events WHERE server_id = 'server-delete'"
                    .to_string(),
            ))
            .await
            .expect("count events")
            .expect("count row")
            .try_get("", "count")
            .expect("event count");
        assert_eq!(offer_count, 0, "credential-bearing offers cascade");
        assert_eq!(event_count, 1, "secret-free authority history is retained");
    }
}
