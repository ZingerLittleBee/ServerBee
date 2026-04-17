use sea_orm::{ConnectionTrait, Database, DatabaseBackend, Statement};
use sea_orm_migration::MigratorTrait;
use serverbee_server::migration::Migrator;

async fn fresh_db() -> sea_orm::DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("connect sqlite");
    // Run everything up through the email-resend migration.
    Migrator::up(&db, None).await.expect("run migrations");
    db
}

async fn exec(db: &sea_orm::DatabaseConnection, sql: &str) {
    db.execute(Statement::from_string(
        DatabaseBackend::Sqlite,
        sql.to_string(),
    ))
    .await
    .expect("exec");
}

async fn config_json_of(db: &sea_orm::DatabaseConnection, id: &str) -> String {
    use sea_orm::FromQueryResult;

    #[derive(FromQueryResult)]
    struct Row {
        config_json: String,
    }

    Row::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT config_json FROM notifications WHERE id = ?",
        [id.into()],
    ))
    .one(db)
    .await
    .expect("query")
    .expect("row")
    .config_json
}

async fn name_of(db: &sea_orm::DatabaseConnection, id: &str) -> String {
    use sea_orm::FromQueryResult;

    #[derive(FromQueryResult)]
    struct Row {
        name: String,
    }

    Row::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT name FROM notifications WHERE id = ?",
        [id.into()],
    ))
    .one(db)
    .await
    .expect("query")
    .expect("row")
    .name
}

async fn enabled_of(db: &sea_orm::DatabaseConnection, id: &str) -> bool {
    use sea_orm::FromQueryResult;

    #[derive(FromQueryResult)]
    struct Row {
        enabled: bool,
    }

    Row::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT enabled FROM notifications WHERE id = ?",
        [id.into()],
    ))
    .one(db)
    .await
    .expect("query")
    .expect("row")
    .enabled
}

#[tokio::test]
async fn migrates_valid_smtp_row_to_resend_schema() {
    // Fresh DB with all migrations already run — insert a legacy-shaped row,
    // then roll our migration back and re-apply to exercise it on the row.
    let db = Database::connect("sqlite::memory:").await.unwrap();

    // Apply all migrations except the last one.
    Migrator::up(&db, Some(Migrator::migrations().len() as u32 - 1))
        .await
        .unwrap();

    exec(
        &db,
        "INSERT INTO notifications (id, name, notify_type, config_json, enabled, created_at) \
         VALUES ('row-1', 'ops email', 'email', \
         '{\"smtp_host\":\"smtp.gmail.com\",\"smtp_port\":587,\"username\":\"u\",\"password\":\"p\",\"from\":\"alerts@x.com\",\"to\":\"ops@y.com\"}', \
         1, '2026-04-16T00:00:00+00:00')",
    )
    .await;

    // Run the final (email-resend) migration.
    Migrator::up(&db, None).await.unwrap();

    let new_json = config_json_of(&db, "row-1").await;
    let v: serde_json::Value = serde_json::from_str(&new_json).unwrap();
    assert_eq!(v["from"], "alerts@x.com");
    assert_eq!(v["to"][0], "ops@y.com");
    assert!(v.get("smtp_host").is_none());
    assert!(enabled_of(&db, "row-1").await, "enabled preserved");
    assert_eq!(name_of(&db, "row-1").await, "ops email");
}

#[tokio::test]
async fn disables_unconvertable_email_row() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&db, Some(Migrator::migrations().len() as u32 - 1))
        .await
        .unwrap();

    // Legacy row missing the `from` field.
    exec(
        &db,
        "INSERT INTO notifications (id, name, notify_type, config_json, enabled, created_at) \
         VALUES ('row-2', 'broken email', 'email', \
         '{\"smtp_host\":\"smtp.gmail.com\",\"to\":\"ops@y.com\"}', \
         1, '2026-04-16T00:00:00+00:00')",
    )
    .await;

    Migrator::up(&db, None).await.unwrap();

    assert!(!enabled_of(&db, "row-2").await, "row should be disabled");
    assert_eq!(
        name_of(&db, "row-2").await,
        "broken email (needs reconfiguration)"
    );
}

#[tokio::test]
async fn empty_table_migrates_without_error() {
    let _ = fresh_db().await;
    // Applying all migrations on an empty DB is a no-op for the email-resend
    // migration; reaching this point without panicking is the assertion.
}
