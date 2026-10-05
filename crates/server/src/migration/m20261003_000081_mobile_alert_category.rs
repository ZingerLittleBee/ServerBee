use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Independent category migrations may run in either integration order.
        // Reuse the shared column without changing existing delivery metadata.
        if !manager.has_column("mobile_push_outbox", "category").await? {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE mobile_push_outbox ADD COLUMN category TEXT NOT NULL DEFAULT 'test'",
                )
                .await?;
        }
        Ok(())
    }
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::Migrator;
    use sea_orm::{ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement};

    async fn accepted_base() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:").await.expect("SQLite");
        let base_index = Migrator::migrations()
            .iter()
            .position(|migration| migration.name() == "m20261003_000080_mobile_push_outbox")
            .expect("accepted base migration");
        let steps = <u32 as std::convert::TryFrom<usize>>::try_from(base_index + 1)
            .expect("migration count");
        Migrator::up(&db, Some(steps))
            .await
            .expect("migrate through accepted outbox base");
        assert!(
            !SchemaManager::new(&db)
                .has_column("mobile_push_outbox", "category")
                .await
                .expect("inspect base schema")
        );
        db.execute_unprepared(
            "INSERT INTO mobile_push_outbox
            (event_id, installation_id, user_id, mobile_session_id, registration_revision,
             recipient_role, created_at, expires_at, envelope, next_attempt_at)
            VALUES ('existing-event', 'existing-installation', 'existing-user', 'existing-session',
                7, 'member', 100, 1900, 'encrypted-fixture', 100)",
        )
        .await
        .expect("seed existing delivery");
        db
    }

    async fn assert_preserved(db: &DatabaseConnection, category: &str) {
        let row = db.query_one(Statement::from_string(DatabaseBackend::Sqlite,
            "SELECT category, envelope, registration_revision FROM mobile_push_outbox WHERE event_id='existing-event'".to_string()))
            .await.expect("read delivery").expect("existing delivery survives");
        assert_eq!(
            row.try_get::<String>("", "category").expect("category"),
            category
        );
        assert_eq!(
            row.try_get::<String>("", "envelope").expect("ciphertext"),
            "encrypted-fixture"
        );
        assert_eq!(
            row.try_get::<i64>("", "registration_revision")
                .expect("revision"),
            7
        );
        let columns = db
            .query_all(Statement::from_string(
                DatabaseBackend::Sqlite,
                "PRAGMA table_info('mobile_push_outbox')".to_string(),
            ))
            .await
            .expect("inspect final schema");
        let category_columns: Vec<_> = columns
            .iter()
            .filter(|column| {
                column.try_get::<String>("", "name").expect("column name") == "category"
            })
            .collect();
        assert_eq!(category_columns.len(), 1);
        assert_eq!(
            category_columns[0]
                .try_get::<String>("", "type")
                .expect("type"),
            "TEXT"
        );
        assert_eq!(
            category_columns[0]
                .try_get::<i64>("", "notnull")
                .expect("not null"),
            1
        );
        assert_eq!(
            category_columns[0]
                .try_get::<String>("", "dflt_value")
                .expect("default"),
            "'test'"
        );
    }

    #[tokio::test]
    async fn mobile_alert_migration_adds_shared_category_preserving_existing_jobs() {
        let db = accepted_base().await;
        Migrator::up(&db, None)
            .await
            .expect("apply category migration");
        assert_preserved(&db, "test").await;
        Migrator::up(&db, None)
            .await
            .expect("restart migration runner");
        assert_preserved(&db, "test").await;
    }

    #[tokio::test]
    async fn mobile_alert_migration_reuses_existing_category_preserving_values() {
        let db = accepted_base().await;
        // Represent another independently reserved migration's shared column.
        db.execute_unprepared(
            "ALTER TABLE mobile_push_outbox ADD COLUMN category TEXT NOT NULL DEFAULT 'test'",
        )
        .await
        .expect("existing shared column");
        db.execute_unprepared("UPDATE mobile_push_outbox SET category='security'")
            .await
            .expect("existing category value");
        Migrator::up(&db, None)
            .await
            .expect("reuse shared category");
        assert_preserved(&db, "security").await;
        Migration
            .up(&SchemaManager::new(&db))
            .await
            .expect("repeat shared-column admission");
        assert_preserved(&db, "security").await;
    }

    #[tokio::test]
    async fn shared_category_and_task_migrations_preserve_both_integration_orders() {
        for task_first in [false, true] {
            let db = accepted_base().await;
            let manager = SchemaManager::new(&db);
            let task = crate::migration::m20261003_000083_task_runs::Migration;
            if task_first {
                task.up(&manager).await.expect("accepted task schema");
                db.execute_unprepared("UPDATE mobile_push_outbox SET task_run_id='queued-run'")
                    .await
                    .expect("existing queued task target");
            }
            Migration.up(&manager).await.expect("shared category");
            if !task_first {
                task.up(&manager).await.expect("task schema after category");
                db.execute_unprepared("UPDATE mobile_push_outbox SET task_run_id='queued-run'")
                    .await
                    .expect("queued task target");
            }
            Migration
                .up(&manager)
                .await
                .expect("guarded category replay");
            assert_preserved(&db, "test").await;
            let row = db
                .query_one(Statement::from_string(
                    DatabaseBackend::Sqlite,
                    "SELECT task_run_id FROM mobile_push_outbox WHERE event_id='existing-event'"
                        .to_string(),
                ))
                .await
                .expect("read target")
                .expect("existing task");
            assert_eq!(
                row.try_get::<String>("", "task_run_id").expect("target"),
                "queued-run"
            );
            for preference in ["alerts", "security", "task_failure", "task_success"] {
                assert!(
                    manager
                        .has_column("mobile_push_registrations", preference)
                        .await
                        .expect("existing preference")
                );
            }
        }
    }
}

#[cfg(test)]
mod combined_category_tests {
    use super::*;
    use crate::migration::Migrator;
    use sea_orm::{Database, DatabaseBackend, Statement};

    #[tokio::test]
    async fn alert_security_and_task_schema_orders_preserve_all_categories_and_preferences() {
        for order in [
            [81, 82, 83],
            [81, 83, 82],
            [82, 81, 83],
            [82, 83, 81],
            [83, 81, 82],
            [83, 82, 81],
        ] {
            let db = Database::connect("sqlite::memory:").await.expect("SQLite");
            let steps = Migrator::migrations()
                .iter()
                .position(|migration| migration.name() == "m20261003_000080_mobile_push_outbox")
                .expect("accepted base")
                + 1;
            Migrator::up(
                &db,
                Some(
                    <u32 as std::convert::TryFrom<usize>>::try_from(steps)
                        .expect("migration count"),
                ),
            )
            .await
            .expect("migrate accepted base");
            let owner = crate::service::auth::AuthService::create_user(
                &db,
                "category-owner",
                "testpass",
                "admin",
            )
            .await
            .expect("real owner");
            crate::service::mobile_auth::MobileAuthService::login_for_user(
                &db,
                &crate::config::MobileConfig::default(),
                &owner,
                "category-install",
                "iPhone",
                "127.0.0.1",
                "fixture",
            )
            .await
            .expect("real mobile session");
            // Query the actual authenticated session instead of inventing FK identities.
            let session = db
                .query_one(Statement::from_sql_and_values(
                    DatabaseBackend::Sqlite,
                    "SELECT id FROM mobile_sessions WHERE installation_id=? AND user_id=?",
                    ["category-install".into(), owner.id.clone().into()],
                ))
                .await
                .expect("session query")
                .expect("session");
            let session_id = session.try_get::<String>("", "id").expect("session id");
            db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite,
                "INSERT INTO mobile_push_registrations (installation_id,user_id,mobile_session_id,revision,enabled,alerts,security,task_failure,task_success,updated_at)
                 VALUES (?,?,?,7,1,1,0,1,0,?)",
                ["category-install".into(), owner.id.into(), session_id.into(), chrono::Utc::now().to_rfc3339().into()]))
                .await.expect("confirmed preferences");
            let manager = SchemaManager::new(&db);
            for sequence in order {
                match sequence {
                    81 => Migration.up(&manager).await,
                    82 => {
                        crate::migration::m20261003_000082_mobile_push_category::Migration
                            .up(&manager)
                            .await
                    }
                    _ => {
                        crate::migration::m20261003_000083_task_runs::Migration
                            .up(&manager)
                            .await
                    }
                }
                .expect("independently accepted schema");
                if sequence == order[0] {
                    // Seed only columns present at this point, as an existing upgrade would.
                    for category in ["test", "alert", "security", "task_failure", "task_success"] {
                        let category_column = sequence != 83;
                        let sql = if category_column {
                            "INSERT INTO mobile_push_outbox (event_id,installation_id,user_id,mobile_session_id,registration_revision,recipient_role,created_at,expires_at,envelope,next_attempt_at,category) VALUES (?,?, 'owner','session',7,'admin',100,1900,'ciphertext',100,?)"
                        } else {
                            "INSERT INTO mobile_push_outbox (event_id,installation_id,user_id,mobile_session_id,registration_revision,recipient_role,created_at,expires_at,envelope,next_attempt_at,task_run_id) VALUES (?,?, 'owner','session',7,'admin',100,1900,'ciphertext',100,?)"
                        };
                        db.execute(Statement::from_sql_and_values(
                            DatabaseBackend::Sqlite,
                            sql,
                            [category.into(), category.into(), category.into()],
                        ))
                        .await
                        .expect("existing category or task target");
                    }
                }
            }
            // Apply both guarded category migrations again to model resumed upgrades.
            Migration.up(&manager).await.expect("repeat alert schema");
            crate::migration::m20261003_000082_mobile_push_category::Migration
                .up(&manager)
                .await
                .expect("repeat security schema");
            let rows = db
                .query_all(Statement::from_string(
                    DatabaseBackend::Sqlite,
                    "SELECT * FROM mobile_push_outbox ORDER BY event_id".to_owned(),
                ))
                .await
                .expect("queue");
            assert_eq!(rows.len(), 5);
            for row in rows {
                let event = row.try_get::<String>("", "event_id").expect("identity");
                assert_eq!(
                    row.try_get::<String>("", "category").expect("category"),
                    if order[0] == 83 { "test" } else { &event }
                );
                assert_eq!(
                    row.try_get::<Option<String>>("", "task_run_id")
                        .expect("task target"),
                    if order[0] == 83 { Some(event) } else { None }
                );
                assert_eq!(
                    row.try_get::<String>("", "envelope").expect("ciphertext"),
                    "ciphertext"
                );
                assert_eq!(
                    row.try_get::<i64>("", "registration_revision")
                        .expect("revision"),
                    7
                );
                assert_eq!(
                    row.try_get::<i64>("", "expires_at").expect("deadline"),
                    1900
                );
            }
            let row = db.query_one(Statement::from_string(DatabaseBackend::Sqlite,
                "SELECT revision,enabled,alerts,security,task_failure,task_success FROM mobile_push_registrations".to_owned()))
                .await.expect("preferences").expect("registration");
            for (column, value) in [
                ("revision", 7),
                ("enabled", 1),
                ("alerts", 1),
                ("security", 0),
                ("task_failure", 1),
                ("task_success", 0),
            ] {
                assert_eq!(
                    row.try_get::<i64>("", column).expect("saved preference"),
                    value
                );
            }
        }
    }
}
