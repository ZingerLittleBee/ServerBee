use crate::migration::Migrator;
use sea_orm::{Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use tempfile::TempDir;

pub async fn setup_test_db() -> (DatabaseConnection, TempDir) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let url = format!("sqlite:{}?mode=rwc", db_path.display());
    let db = Database::connect(&url).await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    (db, tmp)
}
