use sea_orm_migration::prelude::*;

mod m20260312_000001_init;
mod m20260312_000002_oauth;
mod m20260314_000003_add_capabilities;
mod m20260315_000004_network_probe;
mod m20260317_000005_traffic_and_scheduled_tasks;
mod m20260318_000006_docker_support;
mod m20260319_000007_service_monitor;
mod m20260319_000008_disk_io_records;
mod m20260320_000009_status_page;
mod m20260320_000010_dashboard;
mod m20260321_000011_status_page_uptime_thresholds;

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260312_000001_init::Migration),
            Box::new(m20260312_000002_oauth::Migration),
            Box::new(m20260314_000003_add_capabilities::Migration),
            Box::new(m20260315_000004_network_probe::Migration),
            Box::new(m20260317_000005_traffic_and_scheduled_tasks::Migration),
            Box::new(m20260318_000006_docker_support::Migration),
            Box::new(m20260319_000007_service_monitor::Migration),
            Box::new(m20260319_000008_disk_io_records::Migration),
            Box::new(m20260320_000009_status_page::Migration),
            Box::new(m20260320_000010_dashboard::Migration),
            Box::new(m20260321_000011_status_page_uptime_thresholds::Migration),
        ]
    }
}
