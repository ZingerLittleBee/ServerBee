# Configurable Anomaly Thresholds

**Date**: 2026-04-13
**Status**: Approved

## Problem

Network probe anomaly detection thresholds are hardcoded in `crates/server/src/service/network_probe.rs`. Users cannot adjust what constitutes a "warning" or "critical" anomaly for their environment. Different networks have different baseline latencies, so a one-size-fits-all threshold is insufficient.

## Solution

Extend the existing `NetworkProbeSetting` structure with 4 threshold fields. Expose them in the existing settings tab of the network probes settings page. The thresholds are global (apply to all servers uniformly).

## Default Values

| Metric | Warning | Critical |
|--------|---------|----------|
| Latency | > 300 ms | > 800 ms |
| Packet Loss | > 10% | > 50% |

The special `unreachable` anomaly type (packet_loss == 1.0) is always detected regardless of threshold settings.

## Data Model

### Rust DTO (`NetworkProbeSetting`)

```rust
pub struct NetworkProbeSetting {
    // Existing fields
    pub interval: u32,
    pub packet_count: u32,
    pub default_target_ids: Vec<String>,
    // New threshold fields
    pub latency_warn: f64,      // default 300.0 ms
    pub latency_critical: f64,  // default 800.0 ms
    pub loss_warn: f64,         // default 0.1 (10%)
    pub loss_critical: f64,     // default 0.5 (50%)
}
```

### Frontend TypeScript (`NetworkProbeSetting`)

```typescript
export interface NetworkProbeSetting {
  default_target_ids: string[]
  interval: number
  packet_count: number
  latency_warn: number      // ms
  latency_critical: number  // ms
  loss_warn: number         // 0-1
  loss_critical: number     // 0-1
}
```

### Backward Compatibility

Settings are stored as JSON. Use `#[serde(default)]` on the new fields so existing deployments get default values on read without requiring a database migration.

### Validation Rules

- `latency_warn > 0`
- `latency_critical > latency_warn`
- `loss_warn` in (0, 1)
- `loss_critical > loss_warn` and `loss_critical <= 1.0`

## Server-Side Changes

### `get_anomalies()` (network_probe.rs:834)

Replace hardcoded thresholds with values from `NetworkProbeSetting`:

| Current | New |
|---------|-----|
| `latency > 240.0` → very_high_latency | `latency > setting.latency_critical` |
| `latency > 150.0` → high_latency | `latency > setting.latency_warn` |
| `packet_loss > 0.5` → very_high_packet_loss | `packet_loss > setting.loss_critical` |
| `packet_loss > 0.1` → high_packet_loss | `packet_loss > setting.loss_warn` |
| `packet_loss == 1.0` → unreachable | Unchanged |

### `count_anomalies()` (network_probe.rs:1048)

Replace hardcoded SQL values `avg_latency > 150.0` and `packet_loss > 0.1` with parameter-bound `setting.latency_warn` and `setting.loss_warn`.

### Signature Changes

Both `get_anomalies()` and `count_anomalies()` gain a `&NetworkProbeSetting` parameter. Callers (router handlers, summary methods) read the setting once and pass it in.

## Frontend Changes

### UI Location

In `network-probes.tsx`, the existing `settings` tab, below the interval / packet_count / default targets section. No new tab.

### Layout

```
┌─ Anomaly Thresholds ─────────────────────────────────┐
│                                                       │
│  Latency                                              │
│  ┌──────────────┐        ┌──────────────┐            │
│  │  Warning 300 │ ms     │  Critical 800│ ms         │
│  └──────────────┘        └──────────────┘            │
│                                                       │
│  Packet Loss                                          │
│  ┌──────────────┐        ┌──────────────┐            │
│  │  Warning  10 │ %      │  Critical  50│ %          │
│  └──────────────┘        └──────────────┘            │
│                                                       │
└───────────────────────────────────────────────────────┘
```

### Input Details

- 4 `<Input type="number" />` fields, consistent with existing interval/packet_count style
- Packet loss: user enters integer percentage (10, 50), frontend converts to decimal (0.1, 0.5) on submit, converts back to percentage on display
- Client-side validation: warn < critical, values > 0, packet loss ≤ 100%
- Submit merged with existing fields in a single `PUT /api/network-probes/setting` request via `useUpdateNetworkSetting` hook

### i18n

Add translation keys to `en/network.json` and `zh/network.json`:
- `anomaly_thresholds` — section title
- `latency_warn` / `latency_critical` — latency input labels
- `loss_warn` / `loss_critical` — packet loss input labels
- `threshold_warn_must_be_less` — validation error message

## Scope

### In Scope

- Extend `NetworkProbeSetting` with 4 threshold fields (Rust + TypeScript)
- Update `get_anomalies()` and `count_anomalies()` to use configurable thresholds
- Add threshold inputs to settings tab UI
- Add i18n keys (en + zh)
- Update existing tests to cover new thresholds

### Out of Scope

- Per-server threshold overrides
- Per-target threshold overrides
- New database tables or migrations
- New API endpoints or hooks
