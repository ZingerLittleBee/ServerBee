use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260713_000075_agent_authority_lifecycle"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared(
            r#"
            CREATE TABLE enrollment_offers (
                id TEXT PRIMARY KEY NOT NULL,
                code_hash TEXT NOT NULL,
                code_prefix TEXT NOT NULL,
                target_server_id TEXT NOT NULL
                    REFERENCES servers(id) ON DELETE CASCADE,
                created_by TEXT NOT NULL,
                expires_at TIMESTAMP NOT NULL,
                outcome TEXT,
                terminal_at TIMESTAMP,
                successor_offer_id TEXT,
                created_at TIMESTAMP NOT NULL,
                CHECK (outcome IS NULL OR outcome IN ('consumed', 'revoked', 'replaced', 'expired')),
                CHECK (
                    (outcome IS NULL AND terminal_at IS NULL AND successor_offer_id IS NULL)
                    OR
                    (outcome IS NOT NULL AND terminal_at IS NOT NULL)
                ),
                CHECK (
                    (outcome = 'replaced' AND successor_offer_id IS NOT NULL)
                    OR
                    (outcome IS NULL)
                    OR
                    (outcome != 'replaced' AND successor_offer_id IS NULL)
                )
            );

            INSERT INTO enrollment_offers (
                id, code_hash, code_prefix, target_server_id, created_by,
                expires_at, outcome, terminal_at, successor_offer_id, created_at
            )
            SELECT
                id,
                code_hash,
                code_prefix,
                target_server_id,
                created_by,
                expires_at,
                CASE
                    WHEN consumed_at IS NOT NULL THEN 'consumed'
                    WHEN revoked_at IS NOT NULL THEN 'revoked'
                    WHEN expires_at <= CURRENT_TIMESTAMP THEN 'expired'
                    ELSE NULL
                END,
                CASE
                    WHEN consumed_at IS NOT NULL THEN consumed_at
                    WHEN revoked_at IS NOT NULL THEN revoked_at
                    WHEN expires_at <= CURRENT_TIMESTAMP THEN expires_at
                    ELSE NULL
                END,
                NULL,
                created_at
            FROM agent_enrollments;

            DROP TABLE agent_enrollments;

            CREATE UNIQUE INDEX idx_enrollment_offers_outstanding_per_server
                ON enrollment_offers(target_server_id)
                WHERE outcome IS NULL;
            CREATE INDEX idx_enrollment_offers_code_prefix
                ON enrollment_offers(code_prefix);
            CREATE INDEX idx_enrollment_offers_server_created
                ON enrollment_offers(target_server_id, created_at DESC);

            CREATE TABLE agent_authority_events (
                id TEXT PRIMARY KEY NOT NULL,
                server_id TEXT NOT NULL,
                server_name TEXT NOT NULL,
                actor_kind TEXT NOT NULL CHECK (actor_kind IN ('user', 'system', 'agent')),
                actor_id TEXT,
                request_source TEXT NOT NULL,
                offer_id TEXT,
                transition TEXT NOT NULL CHECK (transition IN (
                    'initial_offer_issued', 'offer_issued', 'reenrollment_started',
                    'offer_consumed', 'offer_revoked', 'offer_replaced',
                    'offer_expired', 'authority_revoked', 'server_deleted'
                )),
                mode TEXT CHECK (mode IS NULL OR mode IN ('graceful', 'emergency')),
                offer_outcome TEXT,
                authority_before TEXT NOT NULL
                    CHECK (authority_before IN ('claimed', 'unclaimed')),
                authority_after TEXT NOT NULL
                    CHECK (authority_after IN ('claimed', 'unclaimed')),
                created_at TIMESTAMP NOT NULL,
                CHECK (offer_outcome IS NULL OR offer_outcome IN ('consumed', 'revoked', 'replaced', 'expired'))
            );
            CREATE INDEX idx_agent_authority_events_server_created
                ON agent_authority_events(server_id, created_at DESC);

            CREATE TABLE server_onboarding_requests (
                id TEXT PRIMARY KEY NOT NULL,
                actor_id TEXT NOT NULL,
                request_id TEXT NOT NULL,
                normalized_input_hash TEXT NOT NULL,
                server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                created_at TIMESTAMP NOT NULL,
                UNIQUE(actor_id, request_id)
            );
            CREATE INDEX idx_server_onboarding_requests_server
                ON server_onboarding_requests(server_id);

            ALTER TABLE servers DROP COLUMN fingerprint;
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
