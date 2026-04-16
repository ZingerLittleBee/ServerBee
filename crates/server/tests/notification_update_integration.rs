use sea_orm::Database;
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use serverbee_server::error::AppError;
use serverbee_server::migration::Migrator;
use serverbee_server::service::notification::{
    CreateNotification, NotificationService, UpdateNotification,
};

async fn fresh_db() -> sea_orm::DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("connect sqlite");
    Migrator::up(&db, None).await.expect("run migrations");
    db
}

async fn create_valid_email(db: &sea_orm::DatabaseConnection) -> String {
    let input = CreateNotification {
        name: "ops".to_string(),
        notify_type: "email".to_string(),
        config_json: json!({ "from": "alerts@example.com", "to": ["ops@example.com"] }),
        enabled: true,
    };
    NotificationService::create(db, input)
        .await
        .expect("create baseline email")
        .id
}

#[tokio::test]
async fn update_rejects_empty_to_when_only_config_json_changes() {
    let db = fresh_db().await;
    let id = create_valid_email(&db).await;

    let input = UpdateNotification {
        name: None,
        notify_type: None,
        config_json: Some(json!({ "from": "alerts@example.com", "to": [] })),
        enabled: None,
    };

    let result = NotificationService::update(&db, &id, input).await;
    assert!(matches!(result, Err(AppError::Validation(_))));
}

#[tokio::test]
async fn update_rejects_type_switch_without_matching_config_json() {
    let db = fresh_db().await;
    let id = create_valid_email(&db).await;

    // Change notify_type to telegram but leave the email-shaped config_json alone.
    let input = UpdateNotification {
        name: None,
        notify_type: Some("telegram".to_string()),
        config_json: None,
        enabled: None,
    };

    let result = NotificationService::update(&db, &id, input).await;
    assert!(result.is_err(), "email config should not parse as telegram");
}

#[tokio::test]
async fn update_accepts_valid_merged_payload() {
    let db = fresh_db().await;
    let id = create_valid_email(&db).await;

    let input = UpdateNotification {
        name: Some("renamed".to_string()),
        notify_type: None,
        config_json: None,
        enabled: Some(false),
    };

    let updated = NotificationService::update(&db, &id, input)
        .await
        .expect("valid update");
    assert_eq!(updated.name, "renamed");
    assert!(!updated.enabled);
}
