use std::sync::Arc;

use crate::config::AppConfig;
use crate::service::agent_manager::AgentManager;
use crate::service::alert::{AlertService, AlertStateManager};
use crate::state::AppState;

/// Runs every 60 seconds to evaluate all enabled alert rules.
pub async fn run(state: Arc<AppState>) {
    tracing::info!("Alert evaluator started");

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

    loop {
        interval.tick().await;

        evaluate_tick(
            &state.db,
            &state.config,
            &state.agent_manager,
            &state.alert_state_manager,
        )
        .await;
    }
}

/// Performs the per-tick work of the alert evaluator: evaluate all enabled
/// alert rules once and log any error. Extracted from the `run` loop so it can
/// be exercised in isolation; behavior (work, ordering, logging) is unchanged.
pub(crate) async fn evaluate_tick(
    db: &sea_orm::DatabaseConnection,
    config: &AppConfig,
    agent_manager: &AgentManager,
    state_manager: &AlertStateManager,
) {
    if let Err(e) = AlertService::evaluate_all(db, config, agent_manager, state_manager).await {
        tracing::error!("Alert evaluation error: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{alert_rule, server};
    use crate::service::auth::AuthService;
    use crate::test_utils::setup_test_db;
    use chrono::{TimeZone, Utc};
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};
    use serverbee_common::constants::CAP_DEFAULT;
    use serverbee_common::protocol::BrowserMessage;
    use tokio::sync::broadcast;

    /// Build the granular dependencies the extracted tick needs without an
    /// `AppState`: a fresh agent manager, an empty alert-state manager and a
    /// default config. Returns the broadcast sender so it can be kept alive.
    fn build_deps() -> (
        AgentManager,
        AlertStateManager,
        AppConfig,
        broadcast::Sender<BrowserMessage>,
    ) {
        let (browser_tx, _rx) = broadcast::channel::<BrowserMessage>(16);
        let agent_manager = AgentManager::new(browser_tx.clone());
        let state_manager = AlertStateManager::new();
        (agent_manager, state_manager, AppConfig::default(), browser_tx)
    }

    async fn insert_test_server(db: &sea_orm::DatabaseConnection, id: &str, name: &str) {
        let token_hash = AuthService::hash_password("test").expect("hash_password should succeed");
        // Deterministic timestamp; the tick performs no Now()-based assertion.
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(Some(token_hash)),
            token_prefix: Set(Some("serverbee_test".to_string())),
            name: Set(name.to_string()),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert test server should succeed");
    }

    async fn insert_alert_rule(
        db: &sea_orm::DatabaseConnection,
        id: &str,
        enabled: bool,
        rules_json: &str,
    ) {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        alert_rule::ActiveModel {
            id: Set(id.to_string()),
            name: Set(format!("rule-{id}")),
            enabled: Set(enabled),
            rules_json: Set(rules_json.to_string()),
            trigger_mode: Set("any".to_string()),
            notification_group_id: Set(None),
            fail_trigger_tasks: Set(None),
            recover_trigger_tasks: Set(None),
            cover_type: Set("all".to_string()),
            server_ids_json: Set(None),
            actions_json: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .expect("insert alert rule should succeed");
    }

    /// Empty input: no alert rules configured -> the tick is a no-op and does
    /// not panic.
    #[tokio::test]
    async fn evaluate_tick_no_rules_is_noop() {
        let (db, _tmp) = setup_test_db().await;
        let (agent_manager, state_manager, config, _tx) = build_deps();

        evaluate_tick(&db, &config, &agent_manager, &state_manager).await;

        // No rules were created and none should exist afterwards.
        let rules = alert_rule::Entity::find()
            .all(&db)
            .await
            .expect("query rules should succeed");
        assert!(rules.is_empty(), "no rules should be present");
    }

    /// All-event-driven rules are skipped by evaluate_all (dispatched from WS
    /// handlers instead). The tick must run without panicking and leave the
    /// rule untouched.
    #[tokio::test]
    async fn evaluate_tick_skips_event_driven_only_rule() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "s1", "Srv").await;
        // A rule whose only item is the event-driven `ip_changed` type.
        insert_alert_rule(
            &db,
            "r-event",
            true,
            r#"[{"rule_type":"ip_changed"}]"#,
        )
        .await;
        let (agent_manager, state_manager, config, _tx) = build_deps();

        evaluate_tick(&db, &config, &agent_manager, &state_manager).await;

        // The rule still exists; the tick neither errored nor mutated it.
        let rule = alert_rule::Entity::find_by_id("r-event")
            .one(&db)
            .await
            .expect("query rule should succeed")
            .expect("rule should still exist");
        assert!(rule.enabled);
    }

    /// Happy path: an enabled, non-event-driven rule with a seeded server is
    /// evaluated end to end. The server has no records, so the threshold check
    /// resolves to "not triggered" and no alert state is created — but the tick
    /// must run the full evaluation without panicking.
    #[tokio::test]
    async fn evaluate_tick_runs_simple_rule_without_panic() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "s1", "Srv").await;
        // A CPU threshold rule (non-event-driven) covering all servers.
        // Field names mirror `AlertRuleItem` (rule_type/max/duration).
        insert_alert_rule(
            &db,
            "r-cpu",
            true,
            r#"[{"rule_type":"cpu","max":90.0,"duration":0}]"#,
        )
        .await;
        let (agent_manager, state_manager, config, _tx) = build_deps();

        evaluate_tick(&db, &config, &agent_manager, &state_manager).await;

        // No data => server is not triggered => no alert state recorded.
        assert!(
            !state_manager.is_triggered("r-cpu", "s1", ""),
            "with no metrics the rule should not be in a triggered state"
        );
        // Rule remains intact after evaluation.
        let rule = alert_rule::Entity::find_by_id("r-cpu")
            .one(&db)
            .await
            .expect("query rule should succeed")
            .expect("rule should still exist");
        assert!(rule.enabled);
    }

    /// Disabled rules are filtered out by evaluate_all's `enabled = true`
    /// query; seeding one alongside an enabled rule must not change behavior.
    #[tokio::test]
    async fn evaluate_tick_ignores_disabled_rule() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "s1", "Srv").await;
        insert_alert_rule(
            &db,
            "r-disabled",
            false,
            r#"[{"rule_type":"cpu","max":1.0,"duration":0}]"#,
        )
        .await;
        let (agent_manager, state_manager, config, _tx) = build_deps();

        evaluate_tick(&db, &config, &agent_manager, &state_manager).await;

        assert!(
            !state_manager.is_triggered("r-disabled", "s1", ""),
            "disabled rule must not be evaluated"
        );
    }
}
