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
