use crate::entity::docker_event;
use sea_orm::*;
use serverbee_common::docker_types::DockerEventInfo;

pub struct DockerService;

impl DockerService {
    pub async fn save_event(
        db: &DatabaseConnection,
        server_id: &str,
        event: &DockerEventInfo,
    ) -> Result<(), DbErr> {
        let model = docker_event::ActiveModel {
            server_id: Set(server_id.to_string()),
            timestamp: Set(event.timestamp),
            event_type: Set(event.event_type.clone()),
            action: Set(event.action.clone()),
            actor_id: Set(event.actor_id.clone()),
            actor_name: Set(event.actor_name.clone()),
            attributes: Set(Some(
                serde_json::to_string(&event.attributes).unwrap_or_default(),
            )),
            ..Default::default()
        };
        docker_event::Entity::insert(model).exec(db).await?;
        Ok(())
    }

    pub async fn get_events(
        db: &DatabaseConnection,
        server_id: &str,
        limit: u64,
    ) -> Result<Vec<DockerEventInfo>, DbErr> {
        let events = docker_event::Entity::find()
            .filter(docker_event::Column::ServerId.eq(server_id))
            .order_by_desc(docker_event::Column::Timestamp)
            .limit(limit)
            .all(db)
            .await?;

        Ok(events
            .into_iter()
            .map(|e| DockerEventInfo {
                timestamp: e.timestamp,
                event_type: e.event_type,
                action: e.action,
                actor_id: e.actor_id,
                actor_name: e.actor_name,
                attributes: e
                    .attributes
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default(),
            })
            .collect())
    }

    pub async fn cleanup_expired(
        db: &DatabaseConnection,
        retention_days: u32,
    ) -> Result<u64, DbErr> {
        let cutoff = chrono::Utc::now().timestamp() - (retention_days as i64 * 86400);
        let result = docker_event::Entity::delete_many()
            .filter(docker_event::Column::Timestamp.lt(cutoff))
            .exec(db)
            .await?;
        Ok(result.rows_affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_db;
    use std::collections::HashMap;

    /// Build a DockerEventInfo with the given timestamp and a single attribute.
    fn make_event(timestamp: i64, action: &str) -> DockerEventInfo {
        let mut attributes = HashMap::new();
        attributes.insert("image".to_string(), "nginx:alpine".to_string());
        DockerEventInfo {
            timestamp,
            event_type: "container".to_string(),
            action: action.to_string(),
            actor_id: "actor-123".to_string(),
            actor_name: Some("web".to_string()),
            attributes,
        }
    }

    #[tokio::test]
    async fn save_event_persists_all_fields_and_serializes_attributes() {
        let (db, _tmp) = setup_test_db().await;

        // Saving an event should write a single row with attributes serialized to JSON.
        let event = make_event(1_700_000_000, "start");
        DockerService::save_event(&db, "srv-1", &event)
            .await
            .unwrap();

        let rows = docker_event::Entity::find().all(&db).await.unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.server_id, "srv-1");
        assert_eq!(row.timestamp, 1_700_000_000);
        assert_eq!(row.event_type, "container");
        assert_eq!(row.action, "start");
        assert_eq!(row.actor_id, "actor-123");
        assert_eq!(row.actor_name, Some("web".to_string()));
        // attributes are stored as a JSON string round-trippable back to the map.
        let stored: HashMap<String, String> =
            serde_json::from_str(row.attributes.as_ref().unwrap()).unwrap();
        assert_eq!(stored.get("image"), Some(&"nginx:alpine".to_string()));
    }

    #[tokio::test]
    async fn save_event_handles_empty_attributes_and_none_actor_name() {
        let (db, _tmp) = setup_test_db().await;

        // An event with no attributes and no actor_name should still persist cleanly.
        let event = DockerEventInfo {
            timestamp: 42,
            event_type: "image".to_string(),
            action: "pull".to_string(),
            actor_id: "img-1".to_string(),
            actor_name: None,
            attributes: HashMap::new(),
        };
        DockerService::save_event(&db, "srv-2", &event)
            .await
            .unwrap();

        let row = docker_event::Entity::find().one(&db).await.unwrap().unwrap();
        assert_eq!(row.actor_name, None);
        // Empty map serializes to "{}", not null.
        assert_eq!(row.attributes.as_deref(), Some("{}"));
    }

    #[tokio::test]
    async fn get_events_returns_empty_when_none_match() {
        let (db, _tmp) = setup_test_db().await;

        // No rows for the requested server id yields an empty vec.
        let events = DockerService::get_events(&db, "missing", 10).await.unwrap();
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn get_events_orders_by_timestamp_desc_and_round_trips_attributes() {
        let (db, _tmp) = setup_test_db().await;

        // Insert out of chronological order to verify descending ordering.
        DockerService::save_event(&db, "srv-1", &make_event(100, "start"))
            .await
            .unwrap();
        DockerService::save_event(&db, "srv-1", &make_event(300, "die"))
            .await
            .unwrap();
        DockerService::save_event(&db, "srv-1", &make_event(200, "stop"))
            .await
            .unwrap();

        let events = DockerService::get_events(&db, "srv-1", 10).await.unwrap();
        assert_eq!(events.len(), 3);
        // Newest timestamp first.
        assert_eq!(events[0].timestamp, 300);
        assert_eq!(events[1].timestamp, 200);
        assert_eq!(events[2].timestamp, 100);
        // attributes are deserialized back into the map.
        assert_eq!(
            events[0].attributes.get("image"),
            Some(&"nginx:alpine".to_string())
        );
        assert_eq!(events[0].actor_name, Some("web".to_string()));
    }

    #[tokio::test]
    async fn get_events_respects_limit() {
        let (db, _tmp) = setup_test_db().await;

        // Insert five events; a limit of 2 should return only the two newest.
        for ts in [10_i64, 20, 30, 40, 50] {
            DockerService::save_event(&db, "srv-1", &make_event(ts, "start"))
                .await
                .unwrap();
        }

        let events = DockerService::get_events(&db, "srv-1", 2).await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].timestamp, 50);
        assert_eq!(events[1].timestamp, 40);
    }

    #[tokio::test]
    async fn get_events_filters_by_server_id() {
        let (db, _tmp) = setup_test_db().await;

        // Events for other servers must not leak into a server's result set.
        DockerService::save_event(&db, "srv-a", &make_event(100, "start"))
            .await
            .unwrap();
        DockerService::save_event(&db, "srv-b", &make_event(200, "start"))
            .await
            .unwrap();

        let events = DockerService::get_events(&db, "srv-a", 10).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].timestamp, 100);
    }

    #[tokio::test]
    async fn get_events_falls_back_to_default_for_malformed_attributes() {
        let (db, _tmp) = setup_test_db().await;

        // Directly insert a row whose attributes column is not valid JSON.
        docker_event::ActiveModel {
            server_id: Set("srv-1".to_string()),
            timestamp: Set(123),
            event_type: Set("container".to_string()),
            action: Set("start".to_string()),
            actor_id: Set("actor".to_string()),
            actor_name: Set(None),
            attributes: Set(Some("not-json".to_string())),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        let events = DockerService::get_events(&db, "srv-1", 10).await.unwrap();
        assert_eq!(events.len(), 1);
        // Malformed JSON falls back to an empty attribute map.
        assert!(events[0].attributes.is_empty());
    }

    #[tokio::test]
    async fn get_events_handles_null_attributes_column() {
        let (db, _tmp) = setup_test_db().await;

        // A NULL attributes column should also fall back to an empty map.
        docker_event::ActiveModel {
            server_id: Set("srv-1".to_string()),
            timestamp: Set(123),
            event_type: Set("container".to_string()),
            action: Set("start".to_string()),
            actor_id: Set("actor".to_string()),
            actor_name: Set(None),
            attributes: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        let events = DockerService::get_events(&db, "srv-1", 10).await.unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].attributes.is_empty());
    }

    #[tokio::test]
    async fn cleanup_expired_deletes_only_rows_older_than_cutoff() {
        let (db, _tmp) = setup_test_db().await;

        let now = chrono::Utc::now().timestamp();
        // Old event: well before the 7-day cutoff, should be deleted.
        DockerService::save_event(&db, "srv-1", &make_event(now - 10 * 86_400, "start"))
            .await
            .unwrap();
        // Recent event: inside the retention window, should survive.
        DockerService::save_event(&db, "srv-1", &make_event(now - 1 * 86_400, "stop"))
            .await
            .unwrap();

        let deleted = DockerService::cleanup_expired(&db, 7).await.unwrap();
        assert_eq!(deleted, 1);

        let remaining = docker_event::Entity::find().all(&db).await.unwrap();
        assert_eq!(remaining.len(), 1);
        // The surviving row is the recent one.
        assert_eq!(remaining[0].action, "stop");
    }

    #[tokio::test]
    async fn cleanup_expired_keeps_rows_at_or_after_cutoff_boundary() {
        let (db, _tmp) = setup_test_db().await;

        let now = chrono::Utc::now().timestamp();
        // A timestamp exactly at the cutoff is not `< cutoff`, so it must be kept.
        // retention_days = 1 -> cutoff = now - 86400; insert exactly at cutoff.
        DockerService::save_event(&db, "srv-1", &make_event(now - 86_400, "boundary"))
            .await
            .unwrap();

        let deleted = DockerService::cleanup_expired(&db, 1).await.unwrap();
        // Boundary row is retained (strict less-than comparison).
        assert_eq!(deleted, 0);
        assert_eq!(
            docker_event::Entity::find().all(&db).await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn cleanup_expired_returns_zero_on_empty_table() {
        let (db, _tmp) = setup_test_db().await;

        // Nothing to delete -> rows_affected is 0.
        let deleted = DockerService::cleanup_expired(&db, 30).await.unwrap();
        assert_eq!(deleted, 0);
    }
}
