use sea_orm_migration::prelude::*;

mod m20260312_000001_init;
mod m20260312_000002_oauth;
mod m20260314_000003_add_capabilities;
mod m20260315_000004_network_probe;
mod m20260315_000005_update_builtin_targets;

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260312_000001_init::Migration),
            Box::new(m20260312_000002_oauth::Migration),
            Box::new(m20260314_000003_add_capabilities::Migration),
            Box::new(m20260315_000004_network_probe::Migration),
            Box::new(m20260315_000005_update_builtin_targets::Migration),
        ]
    }
}
