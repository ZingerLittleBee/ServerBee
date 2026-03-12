use sea_orm_migration::prelude::*;

mod m20260312_000001_init;
mod m20260312_000002_oauth;

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260312_000001_init::Migration),
            Box::new(m20260312_000002_oauth::Migration),
        ]
    }
}
