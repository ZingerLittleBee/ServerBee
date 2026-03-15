# Real-time Metrics Chart Design

**Date:** 2026-03-15
**Status:** Approved

## Overview

Add a real-time mode to the server detail page's metrics charts. When active (default), charts display live data accumulated from WebSocket updates in a ring buffer (10 minutes, ~200 data points at 3s intervals). Users can switch between real-time and historical time ranges (1h/6h/24h/7d/30d).

## Architecture

```
Existing WS (useServersWs)
   ↓ BrowserMessage::Update
   ↓ Updates ['servers'] query cache
   ↓
useRealtimeMetrics(serverId) ── subscribes to cache changes ──→ Ring Buffer (10min, ~200 points)
                                                                    ↓
ServerDetailPage ── real-time mode ──→ chartData = buffer
                 ── history mode   ──→ chartData = REST API records
```

No backend changes. Reuses existing `BrowserMessage::Update` messages already pushed at 3s intervals.

## Changes

### New: `apps/web/src/hooks/use-realtime-metrics.ts`

Hook that monitors TanStack Query cache for `['servers']` key changes, extracts the target server's `ServerMetrics`, converts it to a `RealtimeDataPoint`, and appends to a ring buffer.

```typescript
interface RealtimeDataPoint {
  timestamp: string       // ISO string from server's last_active (seconds → ms)
  cpu: number
  memory_pct: number      // computed: (mem_used / mem_total) * 100
  disk_pct: number        // computed: (disk_used / disk_total) * 100
  net_in_speed: number
  net_out_speed: number
  net_in_transfer: number
  net_out_transfer: number
  load1: number
  load5: number
  load15: number
}

function useRealtimeMetrics(serverId: string): RealtimeDataPoint[]
```

Key design decisions:
- **Simplified signature**: No `memTotal`/`diskTotal` parameters needed — `ServerMetrics` from WS already includes `mem_total`, `disk_total`, `mem_used`, `disk_used`. The hook computes percentages directly, avoiding stale-value bugs.
- **No `temperature` field**: `ServerStatus` (the Rust type broadcast via WS) does not include temperature. Temperature chart is hidden in real-time mode, same treatment as GPU.
- **Includes `net_in_transfer`/`net_out_transfer`**: For consistency with historical chart data shape.

Implementation details:

**Deduplication via `last_active`**:
The `['servers']` query cache is modified by multiple WS event types (`full_sync`, `update`, `server_online`, `server_offline`, `capabilities_changed`). Only `update` events carry new metric data. The hook uses the server's `last_active` field (set to `Utc::now().timestamp()` on the server side in `agent_manager.rs` when a new Report arrives) as the deduplication key:
- On each cache change, read the target server's `last_active` from the merged cache
- Compare with the previously recorded `last_active` (stored in a `useRef`)
- Only append a new data point if `last_active` has changed
- This naturally filters out `server_online/offline` and `capabilities_changed` events, which do not update `last_active`

**Timestamp source**:
Use `last_active * 1000` (server-side Unix seconds → JS milliseconds) converted to ISO string via `new Date(last_active * 1000).toISOString()`. This ensures:
- Timestamps reflect actual agent report time, not browser receive time
- No clustering of points due to network jitter
- Consistent with server-side time even if browser clock drifts
- 3s reporting interval provides sufficient resolution at second-level precision

**Seed on mount**:
On initialization, immediately read `queryClient.getQueryData(['servers'])` and, if the target server exists and is online, create an initial data point from the current snapshot. This avoids a 0–3s blank period when first entering real-time mode.

**Ring buffer mechanics**:
- Appends to `useRef<RealtimeDataPoint[]>` array
- When array exceeds ~250 entries, trims to 200 via `slice(-200)` (amortized cost)
- Triggers re-render via `useState` counter increment
- Cleanup: unsubscribe on unmount
- Guards against division by zero when `mem_total` or `disk_total` is 0

### Modified: `apps/web/src/hooks/use-api.ts`

Extend `useServerRecords` to accept an optional `enabled` parameter:

```typescript
function useServerRecords(
  id: string,
  hours: number,
  interval: string,
  options?: { enabled?: boolean }
): UseQueryResult<ServerRecord[]>
```

The `enabled` value is combined with the existing `id.length > 0` check: `enabled: id.length > 0 && (options?.enabled ?? true)`.

### Modified: `apps/web/src/routes/_authed/servers/$id.tsx`

1. Import `useRealtimeMetrics` hook
2. Prepend real-time entry to `TIME_RANGES`:
   ```typescript
   const TIME_RANGES: TimeRange[] = [
     { label: 'Real-time', hours: 0, interval: 'realtime' },
     { label: '1h', hours: 1, interval: 'raw' },
     // ... existing entries
   ]
   ```
3. Default `selectedRange` to `0` (Real-time). No conditional logic based on online status — an empty chart for offline servers is acceptable and avoids complex initialization timing issues (online status depends on WS cache which arrives asynchronously)
4. Conditionally source `chartData`:
   - `interval === 'realtime'` → use `useRealtimeMetrics()` return value
   - Otherwise → use existing REST API `useServerRecords()` data
5. **Disable REST queries in real-time mode**: Pass `{ enabled: !isRealtime }` to `useServerRecords` and add `enabled: !isRealtime` to `gpuRecords` query to avoid unnecessary API calls
6. In real-time mode, hide temperature and GPU charts (data not available in WS)
7. Pass a real-time-specific `formatTime` to `MetricsChart` via closure:
   ```typescript
   const realtimeFormatTime = (time: string) => {
     if (realtimeData.length > 0 && time === realtimeData[0].timestamp) {
       // First data point: full format
       return new Date(time).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
     }
     // Subsequent ticks: short format
     const d = new Date(time)
     return `${String(d.getMinutes()).padStart(2, '0')}:${String(d.getSeconds()).padStart(2, '0')}`
   }
   ```
   This uses closure to capture the data array and compare with the first timestamp — no `MetricsChart` interface change required.

### Modified: `apps/web/src/components/server/metrics-chart.tsx`

No interface changes. The existing `formatTime?: (time: string) => string` prop is sufficient — the parent passes a closure that implements the first-point-vs-subsequent logic.

### Documentation

- `README.md` / `README.zh-CN.md`: Add real-time metrics to feature list
- `CHANGELOG.md`: Add v0.2.0 entry

## Edge Cases

- **Server offline**: Real-time mode is still the default. Chart will be empty until data arrives — this is expected and the user can switch to 1h for historical data
- **Server goes offline while viewing**: Ring buffer retains accumulated data. The chart shows the last window of metrics before disconnect. Data only clears when leaving the page (hook unmount)
- **Tab backgrounded**: Browser may throttle WS; ring buffer will have timestamp gaps. Since timestamps come from server-side `last_active`, the gaps accurately reflect the period when no data was received. Recharts handles discontinuous x-axis data gracefully
- **Page navigation**: Ring buffer lives in hook state; data clears when leaving the page (expected — real-time data is ephemeral)

## What Does NOT Change

- **Backend**: No server-side changes
- **Agent**: Report interval stays at 3s (DEFAULT_REPORT_INTERVAL)
- **WS Protocol**: No new message types, reuses existing `BrowserMessage::Update`
- **Database**: No schema changes
- **record_writer**: Still writes to DB every 60s (independent of real-time display)
