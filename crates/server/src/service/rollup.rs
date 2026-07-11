//! Rollup policy: the single owner of how raw metric records compress over
//! time. Every scalar column of `records` / `records_hourly` is declared once
//! in [`METRIC_COLUMNS`] together with its aggregation function; the hourly
//! rollup SQL is generated from that table, so adding a metric column means
//! adding one descriptor row instead of hand-editing column lists in three
//! places of an SQL string.
//!
//! `disk_io_json` is the documented exception: SQLite cannot aggregate the
//! per-device JSON blob, so `RecordService::aggregate_hourly` folds it in Rust
//! after the SQL pass.

use chrono::{DateTime, Duration, Utc};

use crate::entity::record;

/// Longest range the raw table serves under `interval = "auto"`; longer
/// ranges read the hourly rollup.
pub const RAW_WINDOW_MAX_HOURS: i64 = 24;

/// Which table serves a history query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryTable {
    Raw,
    Hourly,
}

/// Resolve the table for a history query: explicit `"raw"` / `"hourly"` win,
/// anything else (the `"auto"` default) switches to the hourly rollup once
/// the range exceeds [`RAW_WINDOW_MAX_HOURS`].
pub fn select_history_table(interval: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> HistoryTable {
    match interval {
        "raw" => HistoryTable::Raw,
        "hourly" => HistoryTable::Hourly,
        _ => {
            if to - from > Duration::hours(RAW_WINDOW_MAX_HOURS) {
                HistoryTable::Hourly
            } else {
                HistoryTable::Raw
            }
        }
    }
}

/// How a raw metric column folds into its hourly bucket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RollupAgg {
    /// `AVG(col)` — REAL gauges keep fractional precision.
    Avg,
    /// `CAST(AVG(col) AS INTEGER)` — integer gauges.
    AvgInt,
    /// `CAST(MAX(col) AS INTEGER)` — cumulative counters keep the window
    /// maximum (equal to the window-end value while the counter grows
    /// monotonically) instead of a meaningless average.
    MaxInt,
}

impl RollupAgg {
    fn select_expr(self, column: &str) -> String {
        match self {
            RollupAgg::Avg => format!("AVG({column})"),
            RollupAgg::AvgInt => format!("CAST(AVG({column}) AS INTEGER)"),
            RollupAgg::MaxInt => format!("CAST(MAX({column}) AS INTEGER)"),
        }
    }
}

/// One rollup-managed metric column, shared by `records` and `records_hourly`
/// (the two tables have identical column sets).
pub struct MetricColumn {
    /// SQL column name in both tables.
    pub name: &'static str,
    /// How the raw samples fold into the hourly bucket.
    pub agg: RollupAgg,
    /// Alert `rule_type` that reads this column; `None` marks columns that
    /// cannot be alerted on (cumulative transfer counters).
    pub alert_rule_type: Option<&'static str>,
    /// Typed accessor backing the alert read path; nullable columns read 0.
    pub read: fn(&record::Model) -> f64,
}

/// The scalar metric columns, in physical column order. Order matters: the
/// generated INSERT lists columns in this order.
pub const METRIC_COLUMNS: &[MetricColumn] = &[
    MetricColumn {
        name: "cpu",
        agg: RollupAgg::Avg,
        alert_rule_type: Some("cpu"),
        read: |r| r.cpu,
    },
    MetricColumn {
        name: "mem_used",
        agg: RollupAgg::AvgInt,
        alert_rule_type: Some("memory"),
        read: |r| r.mem_used as f64,
    },
    MetricColumn {
        name: "swap_used",
        agg: RollupAgg::AvgInt,
        alert_rule_type: Some("swap"),
        read: |r| r.swap_used as f64,
    },
    MetricColumn {
        name: "disk_used",
        agg: RollupAgg::AvgInt,
        alert_rule_type: Some("disk"),
        read: |r| r.disk_used as f64,
    },
    MetricColumn {
        name: "net_in_speed",
        agg: RollupAgg::AvgInt,
        alert_rule_type: Some("net_in_speed"),
        read: |r| r.net_in_speed as f64,
    },
    MetricColumn {
        name: "net_out_speed",
        agg: RollupAgg::AvgInt,
        alert_rule_type: Some("net_out_speed"),
        read: |r| r.net_out_speed as f64,
    },
    MetricColumn {
        name: "net_in_transfer",
        agg: RollupAgg::MaxInt,
        alert_rule_type: None,
        read: |r| r.net_in_transfer as f64,
    },
    MetricColumn {
        name: "net_out_transfer",
        agg: RollupAgg::MaxInt,
        alert_rule_type: None,
        read: |r| r.net_out_transfer as f64,
    },
    MetricColumn {
        name: "load1",
        agg: RollupAgg::Avg,
        alert_rule_type: Some("load1"),
        read: |r| r.load1,
    },
    MetricColumn {
        name: "load5",
        agg: RollupAgg::Avg,
        alert_rule_type: Some("load5"),
        read: |r| r.load5,
    },
    MetricColumn {
        name: "load15",
        agg: RollupAgg::Avg,
        alert_rule_type: Some("load15"),
        read: |r| r.load15,
    },
    MetricColumn {
        name: "tcp_conn",
        agg: RollupAgg::AvgInt,
        alert_rule_type: Some("tcp_conn"),
        read: |r| r.tcp_conn as f64,
    },
    MetricColumn {
        name: "udp_conn",
        agg: RollupAgg::AvgInt,
        alert_rule_type: Some("udp_conn"),
        read: |r| r.udp_conn as f64,
    },
    MetricColumn {
        name: "process_count",
        agg: RollupAgg::AvgInt,
        alert_rule_type: Some("process"),
        read: |r| r.process_count as f64,
    },
    MetricColumn {
        name: "temperature",
        agg: RollupAgg::Avg,
        alert_rule_type: Some("temperature"),
        read: |r| r.temperature.unwrap_or(0.0),
    },
    MetricColumn {
        name: "gpu_usage",
        agg: RollupAgg::Avg,
        alert_rule_type: Some("gpu"),
        read: |r| r.gpu_usage.unwrap_or(0.0),
    },
];

/// Metric value an alert rule reads from a raw record. `None` for unknown
/// rule types and the deliberately non-alertable columns (`alert_rule_type:
/// None`), so such rules cannot fire — not even a `min <= 0` threshold that
/// a 0.0 fallback would satisfy.
pub fn alert_metric(rec: &record::Model, rule_type: &str) -> Option<f64> {
    METRIC_COLUMNS
        .iter()
        .find(|c| c.alert_rule_type == Some(rule_type))
        .map(|c| (c.read)(rec))
}

/// Build the hourly rollup upsert. Placeholders: bucket time, window start,
/// window end (in that order).
pub fn aggregate_hourly_sql() -> String {
    let insert_columns = METRIC_COLUMNS
        .iter()
        .map(|c| c.name)
        .collect::<Vec<_>>()
        .join(", ");
    let select_exprs = METRIC_COLUMNS
        .iter()
        .map(|c| c.agg.select_expr(c.name))
        .collect::<Vec<_>>()
        .join(", ");
    let conflict_sets = METRIC_COLUMNS
        .iter()
        .map(|c| format!("{0} = excluded.{0}", c.name))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "INSERT INTO records_hourly (server_id, time, {insert_columns}) \
         SELECT server_id, ?, {select_exprs} \
         FROM records WHERE time >= ? AND time < ? \
         GROUP BY server_id \
         ON CONFLICT(server_id, time) DO UPDATE SET {conflict_sets}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden test: the generated statement must stay byte-for-byte identical
    /// to the hand-written SQL it replaced, so the descriptor migration cannot
    /// change rollup semantics.
    #[test]
    fn aggregate_hourly_sql_matches_the_original_hand_written_statement() {
        let expected = "INSERT INTO records_hourly \
            (server_id, time, cpu, mem_used, swap_used, disk_used, \
             net_in_speed, net_out_speed, net_in_transfer, net_out_transfer, \
             load1, load5, load15, tcp_conn, udp_conn, process_count, \
             temperature, gpu_usage) \
            SELECT \
                server_id, \
                ?, \
                AVG(cpu), \
                CAST(AVG(mem_used) AS INTEGER), \
                CAST(AVG(swap_used) AS INTEGER), \
                CAST(AVG(disk_used) AS INTEGER), \
                CAST(AVG(net_in_speed) AS INTEGER), \
                CAST(AVG(net_out_speed) AS INTEGER), \
                CAST(MAX(net_in_transfer) AS INTEGER), \
                CAST(MAX(net_out_transfer) AS INTEGER), \
                AVG(load1), \
                AVG(load5), \
                AVG(load15), \
                CAST(AVG(tcp_conn) AS INTEGER), \
                CAST(AVG(udp_conn) AS INTEGER), \
                CAST(AVG(process_count) AS INTEGER), \
                AVG(temperature), \
                AVG(gpu_usage) \
            FROM records \
            WHERE time >= ? AND time < ? \
            GROUP BY server_id \
            ON CONFLICT(server_id, time) DO UPDATE SET \
                cpu = excluded.cpu, \
                mem_used = excluded.mem_used, \
                swap_used = excluded.swap_used, \
                disk_used = excluded.disk_used, \
                net_in_speed = excluded.net_in_speed, \
                net_out_speed = excluded.net_out_speed, \
                net_in_transfer = excluded.net_in_transfer, \
                net_out_transfer = excluded.net_out_transfer, \
                load1 = excluded.load1, \
                load5 = excluded.load5, \
                load15 = excluded.load15, \
                tcp_conn = excluded.tcp_conn, \
                udp_conn = excluded.udp_conn, \
                process_count = excluded.process_count, \
                temperature = excluded.temperature, \
                gpu_usage = excluded.gpu_usage";
        assert_eq!(aggregate_hourly_sql(), expected);
    }

    /// Table selection: explicit intervals win; "auto" switches to hourly
    /// strictly beyond 24h (a range of exactly 24h still reads raw).
    #[test]
    fn select_history_table_covers_explicit_and_auto_intervals() {
        let now = Utc::now();
        let hours = |h: i64| now - Duration::hours(h);

        assert_eq!(select_history_table("raw", hours(48), now), HistoryTable::Raw);
        assert_eq!(
            select_history_table("hourly", hours(1), now),
            HistoryTable::Hourly
        );
        assert_eq!(select_history_table("auto", hours(12), now), HistoryTable::Raw);
        assert_eq!(
            select_history_table("auto", hours(RAW_WINDOW_MAX_HOURS), now),
            HistoryTable::Raw
        );
        assert_eq!(
            select_history_table("auto", hours(48), now),
            HistoryTable::Hourly
        );
    }

    #[test]
    fn metric_columns_have_unique_names() {
        let mut names: Vec<_> = METRIC_COLUMNS.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), METRIC_COLUMNS.len());
    }

    fn make_record(cpu: f64, mem_used: i64, load1: f64) -> record::Model {
        record::Model {
            id: 1,
            server_id: "srv-1".to_string(),
            time: Utc::now(),
            cpu,
            mem_used,
            swap_used: 0,
            disk_used: 0,
            net_in_speed: 0,
            net_out_speed: 0,
            net_in_transfer: 0,
            net_out_transfer: 0,
            load1,
            load5: 0.0,
            load15: 0.0,
            tcp_conn: 100,
            udp_conn: 50,
            process_count: 200,
            temperature: Some(55.0),
            gpu_usage: Some(40.0),
            disk_io_json: None,
        }
    }

    #[track_caller]
    fn assert_metric(rec: &record::Model, rule_type: &str, expected: f64) {
        let value = alert_metric(rec, rule_type).expect("alertable rule type");
        assert!((value - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn alert_metric_reads_direct_columns() {
        let rec = make_record(85.5, 4_000_000, 1.2);
        assert_metric(&rec, "cpu", 85.5);
        assert_metric(&rec, "load1", 1.2);
    }

    /// Aliased rule types ("memory", "process", "gpu"…) resolve through the
    /// descriptor's alert_rule_type, not the column name.
    #[test]
    fn alert_metric_resolves_rule_type_aliases() {
        let rec = make_record(50.0, 8_000_000, 0.0);
        assert_metric(&rec, "memory", 8_000_000.0);
        assert_metric(&rec, "process", 200.0);
        assert_metric(&rec, "gpu", 40.0);
        assert_metric(&rec, "tcp_conn", 100.0);
        assert_metric(&rec, "udp_conn", 50.0);
        assert_metric(&rec, "temperature", 55.0);
    }

    /// Unknown rule types and the deliberately non-alertable transfer
    /// counters resolve to `None` so such rules can never fire — even a
    /// degenerate `min <= 0` threshold gets no value to compare against.
    #[test]
    fn alert_metric_unknown_and_non_alertable_resolve_to_none() {
        let rec = make_record(99.0, 0, 0.0);
        assert_eq!(alert_metric(&rec, "nonexistent"), None);
        assert_eq!(alert_metric(&rec, "net_in_transfer"), None);
        assert_eq!(alert_metric(&rec, "net_out_transfer"), None);
    }

    #[test]
    fn alert_metric_remaining_variants() {
        // Exercise every alertable column not covered by the tests above.
        let mut rec = make_record(0.0, 0, 0.0);
        rec.swap_used = 1024;
        rec.disk_used = 2048;
        rec.load5 = 2.5;
        rec.load15 = 3.5;
        rec.net_in_speed = 111;
        rec.net_out_speed = 222;
        assert_metric(&rec, "swap", 1024.0);
        assert_metric(&rec, "disk", 2048.0);
        assert_metric(&rec, "load5", 2.5);
        assert_metric(&rec, "load15", 3.5);
        assert_metric(&rec, "net_in_speed", 111.0);
        assert_metric(&rec, "net_out_speed", 222.0);
    }

    #[test]
    fn alert_metric_none_temperature_and_gpu_read_zero() {
        // Absent sensors are an alertable column reading 0.0 (Some), not an
        // unknown rule type (None).
        let mut rec = make_record(0.0, 0, 0.0);
        rec.temperature = None;
        rec.gpu_usage = None;
        assert_metric(&rec, "temperature", 0.0);
        assert_metric(&rec, "gpu", 0.0);
    }
}
