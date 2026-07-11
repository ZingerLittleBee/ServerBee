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
    /// `CAST(MAX(col) AS INTEGER)` — cumulative counters keep the window-end
    /// value instead of a meaningless average.
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
}

/// The scalar metric columns, in physical column order. Order matters: the
/// generated INSERT lists columns in this order.
pub const METRIC_COLUMNS: &[MetricColumn] = &[
    MetricColumn { name: "cpu", agg: RollupAgg::Avg },
    MetricColumn { name: "mem_used", agg: RollupAgg::AvgInt },
    MetricColumn { name: "swap_used", agg: RollupAgg::AvgInt },
    MetricColumn { name: "disk_used", agg: RollupAgg::AvgInt },
    MetricColumn { name: "net_in_speed", agg: RollupAgg::AvgInt },
    MetricColumn { name: "net_out_speed", agg: RollupAgg::AvgInt },
    MetricColumn { name: "net_in_transfer", agg: RollupAgg::MaxInt },
    MetricColumn { name: "net_out_transfer", agg: RollupAgg::MaxInt },
    MetricColumn { name: "load1", agg: RollupAgg::Avg },
    MetricColumn { name: "load5", agg: RollupAgg::Avg },
    MetricColumn { name: "load15", agg: RollupAgg::Avg },
    MetricColumn { name: "tcp_conn", agg: RollupAgg::AvgInt },
    MetricColumn { name: "udp_conn", agg: RollupAgg::AvgInt },
    MetricColumn { name: "process_count", agg: RollupAgg::AvgInt },
    MetricColumn { name: "temperature", agg: RollupAgg::Avg },
    MetricColumn { name: "gpu_usage", agg: RollupAgg::Avg },
];

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
}
