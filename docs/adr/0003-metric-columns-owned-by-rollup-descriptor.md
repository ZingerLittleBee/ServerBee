# ADR-0003: Scalar metric columns are owned by the rollup descriptor

## Status

Accepted (2026-07-11)

## Context

A scalar metric's knowledge was re-encoded in several places that nothing
cross-checked: the hourly rollup SQL string listed every column two to three
times and was the only place the AVG-vs-MAX aggregation decision lived;
`query_history` hardcoded the 24h raw/hourly switch; alert evaluation kept a
private read path (`check_threshold` querying the records table directly) plus
a second rule-type→column `match`. Adding one metric column meant hand-editing
~16 sites across 9 files, and the two easiest sites to miss (the SQL string,
the public DTO maps) had no compiler protection.

## Decision

`service::rollup` owns the rollup policy. Every scalar column of
`records`/`records_hourly` is declared once in `METRIC_COLUMNS` with its SQL
name, `RollupAgg` (AVG / CAST-AVG / window MAX — for a monotonically growing
counter the maximum coincides with the window-end value), optional alert rule
type, and a typed accessor. From that table we derive:

- the hourly rollup upsert (`aggregate_hourly_sql()`), pinned byte-for-byte to
  the original hand-written SQL by a golden test;
- the alert metric read (`alert_metric` → `Option<f64>`), with transfer
  counters deliberately non-alertable: `alert_rule_type: None` resolves to
  `None`, and `check_threshold` skips samples without a value, so no threshold
  shape (including `min <= 0`) can fire on them;
- the raw/hourly switch (`select_history_table`, `RAW_WINDOW_MAX_HOURS`).

Alert evaluation reads through `RecordService::query_recent` /
`latest_record_time` instead of querying tables directly, so a storage change
cannot silently detach alerting from what the recorder writes. Because
`records_hourly` mirrors the `records` column set, `record_hourly::Model`
converts into `record::Model` and `QueryHistoryResult::into_rows()` erases the
resolution for consumers that don't care (the public metrics DTO maps once,
not per branch).

On the web, chart display specs (dataKey, color, unit, domain, byte
formatting, availability gate) live in `METRIC_CHART_SPECS` and render through
one loop.

## Alternatives considered

- **Proc-macro / build-script codegen of the structs themselves** (SystemReport,
  entities, DTOs from one schema): kills the remaining duplication but adds a
  compile-time machinery cost far above the ~yearly rate of adding metrics.
  The struct field lists are already compiler-checked; only the SQL and
  string-keyed maps were not.
- **Deriving the web metric list from the OpenAPI schema**: the generated
  `api-types.ts` already mirrors field names/types; what the charts need is
  display knowledge (color, unit, gate) that OpenAPI does not carry, so a
  hand-written display descriptor is the honest owner.

## Consequences

- Adding a scalar metric: one `METRIC_COLUMNS` row + entity/migration +
  protocol/DTO fields (compiler-enforced) + one `METRIC_CHART_SPECS` row and
  i18n keys for a chart. The golden SQL test must be updated deliberately,
  which is the point — aggregation choices are reviewed, not implied.
- `disk_io_json` stays the documented exception (per-device JSON folded in
  Rust after the SQL pass); GPU detail keeps its own sub-table with no rollup.
- The descriptor's accessor field ties it to `record::Model`; if raw storage
  ever splits from the entity shape, the descriptor is the single seam to
  re-point.
