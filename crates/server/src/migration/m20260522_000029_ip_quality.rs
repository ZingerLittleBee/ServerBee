use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260522_000029_ip_quality"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS unlock_service (
                id TEXT PRIMARY KEY NOT NULL,
                key TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                category TEXT NOT NULL,
                popularity INTEGER NOT NULL DEFAULT 0,
                is_builtin INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1,
                detector TEXT,
                request TEXT,
                rules TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS unlock_result (
                id TEXT PRIMARY KEY NOT NULL,
                server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                service_id TEXT NOT NULL REFERENCES unlock_service(id) ON DELETE CASCADE,
                status TEXT NOT NULL,
                region TEXT,
                latency_ms INTEGER,
                detail TEXT,
                checked_at TEXT NOT NULL,
                UNIQUE(server_id, service_id)
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS unlock_event (
                id TEXT PRIMARY KEY NOT NULL,
                server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                service_id TEXT NOT NULL REFERENCES unlock_service(id) ON DELETE CASCADE,
                old_status TEXT NOT NULL,
                new_status TEXT NOT NULL,
                changed_at TEXT NOT NULL
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_unlock_event_server_changed ON unlock_event (server_id, changed_at)",
        )
        .await?;

        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS ip_quality_snapshot (
                id TEXT PRIMARY KEY NOT NULL,
                server_id TEXT NOT NULL UNIQUE REFERENCES servers(id) ON DELETE CASCADE,
                ip TEXT NOT NULL,
                asn TEXT,
                as_org TEXT,
                country TEXT,
                region TEXT,
                city TEXT,
                ip_type TEXT NOT NULL DEFAULT 'unknown',
                is_proxy INTEGER NOT NULL DEFAULT 0,
                is_vpn INTEGER NOT NULL DEFAULT 0,
                is_hosting INTEGER NOT NULL DEFAULT 0,
                risk_score INTEGER,
                risk_level TEXT NOT NULL DEFAULT 'unknown',
                checked_at TEXT NOT NULL
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS ip_risk_cache (
                ip TEXT PRIMARY KEY NOT NULL,
                asn TEXT,
                as_org TEXT,
                country TEXT,
                region TEXT,
                city TEXT,
                ip_type TEXT NOT NULL DEFAULT 'unknown',
                is_proxy INTEGER NOT NULL DEFAULT 0,
                is_vpn INTEGER NOT NULL DEFAULT 0,
                is_hosting INTEGER NOT NULL DEFAULT 0,
                risk_score INTEGER,
                risk_level TEXT NOT NULL DEFAULT 'unknown',
                providers TEXT NOT NULL DEFAULT '{}',
                checked_at TEXT NOT NULL
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS ip_quality_setting (
                id TEXT PRIMARY KEY NOT NULL,
                check_interval_hours INTEGER NOT NULL DEFAULT 12
            )",
        )
        .await?;

        // Insert default settings row.
        db.execute_unprepared(
            "INSERT INTO ip_quality_setting (id, check_interval_hours) VALUES ('default', 12)",
        )
        .await?;

        // Insert built-in service catalog seed. Each statement is a compile-time
        // string literal so the migration stays deterministic and immutable.
        // Columns: id, key, name, category, popularity, is_builtin, enabled,
        // detector, request, rules, created_at, updated_at.
        db.execute_unprepared(
            "INSERT INTO unlock_service (id, key, name, category, popularity, is_builtin, enabled, detector, request, rules, created_at, updated_at) \
             VALUES ('01960000-0000-7000-8000-000000000001', 'netflix', 'Netflix', 'streaming', 100, 1, 1, 'netflix', NULL, NULL, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z')",
        )
        .await?;
        db.execute_unprepared(
            "INSERT INTO unlock_service (id, key, name, category, popularity, is_builtin, enabled, detector, request, rules, created_at, updated_at) \
             VALUES ('01960000-0000-7000-8000-000000000002', 'disney_plus', 'Disney+', 'streaming', 95, 1, 1, 'disney_plus', NULL, NULL, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z')",
        )
        .await?;
        db.execute_unprepared(
            "INSERT INTO unlock_service (id, key, name, category, popularity, is_builtin, enabled, detector, request, rules, created_at, updated_at) \
             VALUES ('01960000-0000-7000-8000-000000000003', 'youtube_premium', 'YouTube Premium', 'streaming', 90, 1, 1, 'youtube_premium', NULL, NULL, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z')",
        )
        .await?;
        db.execute_unprepared(
            "INSERT INTO unlock_service (id, key, name, category, popularity, is_builtin, enabled, detector, request, rules, created_at, updated_at) \
             VALUES ('01960000-0000-7000-8000-000000000004', 'amazon_prime', 'Amazon Prime Video', 'streaming', 80, 1, 1, 'amazon_prime', NULL, NULL, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z')",
        )
        .await?;
        db.execute_unprepared(
            "INSERT INTO unlock_service (id, key, name, category, popularity, is_builtin, enabled, detector, request, rules, created_at, updated_at) \
             VALUES ('01960000-0000-7000-8000-000000000005', 'hbo_max', 'HBO Max', 'streaming', 70, 1, 1, 'hbo_max', NULL, NULL, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z')",
        )
        .await?;
        db.execute_unprepared(
            "INSERT INTO unlock_service (id, key, name, category, popularity, is_builtin, enabled, detector, request, rules, created_at, updated_at) \
             VALUES ('01960000-0000-7000-8000-000000000006', 'chatgpt', 'ChatGPT', 'ai', 100, 1, 1, 'chatgpt', NULL, NULL, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z')",
        )
        .await?;
        db.execute_unprepared(
            "INSERT INTO unlock_service (id, key, name, category, popularity, is_builtin, enabled, detector, request, rules, created_at, updated_at) \
             VALUES ('01960000-0000-7000-8000-000000000007', 'gemini', 'Google Gemini', 'ai', 85, 1, 1, 'gemini', NULL, NULL, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z')",
        )
        .await?;
        db.execute_unprepared(
            "INSERT INTO unlock_service (id, key, name, category, popularity, is_builtin, enabled, detector, request, rules, created_at, updated_at) \
             VALUES ('01960000-0000-7000-8000-000000000008', 'spotify', 'Spotify', 'social', 80, 1, 1, 'spotify', NULL, NULL, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z')",
        )
        .await?;
        db.execute_unprepared(
            "INSERT INTO unlock_service (id, key, name, category, popularity, is_builtin, enabled, detector, request, rules, created_at, updated_at) \
             VALUES ('01960000-0000-7000-8000-000000000009', 'tiktok', 'TikTok', 'social', 85, 1, 1, 'tiktok', NULL, NULL, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z')",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
