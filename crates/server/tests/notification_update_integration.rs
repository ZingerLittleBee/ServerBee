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

#[tokio::test]
async fn update_reenables_disabled_email_row_after_reconfig() {
    let db = fresh_db().await;

    // Simulate the post-migration state: disabled email row with a legacy-ish name.
    let input = CreateNotification {
        name: "ops (needs reconfiguration)".to_string(),
        notify_type: "email".to_string(),
        config_json: json!({ "from": "alerts@example.com", "to": ["ops@example.com"] }),
        enabled: false,
    };
    let id = NotificationService::create(&db, input)
        .await
        .expect("create disabled email")
        .id;

    // The user fills in new resend-shaped config and flips enabled=true in the UI.
    let patch = UpdateNotification {
        name: Some("ops".to_string()),
        notify_type: None,
        config_json: Some(json!({
            "from": "alerts@example.com",
            "to": ["new@example.com", "oncall@example.com"],
        })),
        enabled: Some(true),
    };

    let updated = NotificationService::update(&db, &id, patch)
        .await
        .expect("valid email re-config");
    assert_eq!(updated.name, "ops");
    assert!(updated.enabled, "disabled row should be re-enabled");
    let cfg: serde_json::Value = serde_json::from_str(&updated.config_json).unwrap();
    assert_eq!(cfg["to"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn update_can_toggle_enabled_alone() {
    let db = fresh_db().await;
    let id = NotificationService::create(
        &db,
        CreateNotification {
            name: "ops".to_string(),
            notify_type: "email".to_string(),
            config_json: json!({ "from": "a@x.com", "to": ["b@x.com"] }),
            enabled: false,
        },
    )
    .await
    .expect("create")
    .id;

    let updated = NotificationService::update(
        &db,
        &id,
        UpdateNotification {
            name: None,
            notify_type: None,
            config_json: None,
            enabled: Some(true),
        },
    )
    .await
    .expect("toggle enabled");
    assert!(updated.enabled);
}
