# GeoIP Download-Based Country Lookup Design

## Problem

Server Map widget always shows "No server location data available" because `country_code` is never populated. The existing GeoIP system requires manual MMDB configuration (`geoip.enabled` + `geoip.mmdb_path`), which is disabled by default. A temporary ip-api.com online fallback was added but is unreliable (rate limits, privacy, third-party dependency).

## Solution

Replace the online fallback with a one-click download flow. Users click a button in the UI to download the free DB-IP Lite Country MMDB (~1.5 MB) from the official CDN. The file is saved to the server's data directory and hot-loaded without restart.

## Data Source

- **Database**: [DB-IP Lite Country](https://db-ip.com/db/lite.php) MMDB format
- **License**: CC BY 4.0 (free for commercial use with attribution)
- **Size**: ~1.5 MB (country-only, no city/region bloat)
- **URL pattern**: `https://download.db-ip.com/free/dbip-country-lite-{YYYY-MM}.mmdb.gz`
- **Update cadence**: Published monthly on the 1st
- **Precision**: Country-level (ISO 3166-1 alpha-2), matches Server Map widget needs exactly

## Architecture

### GeoIpService Changes

```rust
// AppState — uses std::sync::RwLock (not tokio), since lookup() is synchronous
// and write lock is held only for nanoseconds during hot-load replacement
pub geoip: Arc<std::sync::RwLock<Option<GeoIpService>>>

// GeoIpService — inner Arc removed (outer Arc<RwLock> handles sharing)
pub struct GeoIpService {
    reader: Reader<Vec<u8>>,
}
```

**Startup behavior** (in `AppState::new()`):
1. If `geoip.mmdb_path` is non-empty and file exists → load that (user override, higher precision)
2. Else if `{data_dir}/dbip-country-lite.mmdb` exists → load that (previously downloaded)
3. Else → `None` (GeoIP not available, widget shows download prompt)

**Hot-load**: After download completes, acquire write lock and replace the `Option<GeoIpService>`.

### Config Changes

```rust
pub struct GeoIpConfig {
    // `enabled` field REMOVED — GeoIP is always available when a database exists
    #[serde(default)]
    pub mmdb_path: String,  // Optional override path, empty = use data directory
}
```

Backward compatible: `serde(default)` on the struct, and `#[serde(deny_unknown_fields)]` is not used, so old configs with `enabled = true` are silently ignored.

### API Endpoints

```
GET  /api/geoip/status    → { installed: bool, file_size?: i64, updated_at?: string }
POST /api/geoip/download  → { success: bool, message: string }
```

Both endpoints require admin role.

**Download endpoint flow**:
1. Check `AtomicBool` download guard — if already in progress, return `{ success: false, message: "Download already in progress" }`
2. Build URL: `https://download.db-ip.com/free/dbip-country-lite-{YYYY-MM}.mmdb.gz`
3. Download with `reqwest::get().bytes().await` (full download into memory, ~1.5 MB)
4. Decompress gzip with `flate2::read::GzDecoder`
5. `fs::create_dir_all(data_dir)` to ensure directory exists
6. Write to `{data_dir}/dbip-country-lite.mmdb.tmp`, then `fs::rename()` to final path (atomic replacement)
7. Hot-load into `Arc<RwLock<Option<GeoIpService>>>`
8. Clear download guard
9. Return success/error

**Download error responses** (all return `{ success: false, message: "..." }`):
- Network error → "Failed to download: connection error"
- Non-200 HTTP status → "Failed to download: server returned {status}"
- Gzip decompression failure → "Failed to decompress database file"
- Disk write / permission error → "Failed to save database: {error}"
- MMDB parse failure → "Downloaded file is not a valid MMDB database"

**Concurrent download guard**: `AtomicBool` on `AppState`, checked at endpoint entry. Prevents races from double-clicks or multiple admins.

**Status endpoint**: Check file existence at data_dir path, `fs::metadata()` for size and mtime.

### Agent-Side Call Chain

**Improvement over current behavior**: The current SystemInfo handler resolves GeoIP from `remote_addr` only. The new code prefers the agent's reported public IP (`info.ipv4`/`info.ipv6`), which is more accurate when agents are behind NAT/proxy. Falls back to `remote_addr` if no reported IP.

**agent.rs SystemInfo handler** (simplified):
```rust
let ip = info.ipv4.as_deref()
    .or(info.ipv6.as_deref())
    .and_then(|ip| ip.parse().ok())
    .or_else(|| state.agent_manager.get_remote_addr(server_id).map(|a| a.ip()));

let (region, country_code) = match ip {
    Some(ip) => {
        let guard = state.geoip.read().unwrap();
        match guard.as_ref() {
            Some(g) => {
                let geo = g.lookup(ip);
                (geo.region, geo.country_code)
            }
            None => (None, None),
        }
    }
    None => (None, None),
};
```

IP change handler follows the same pattern.

**Agent reporter.rs**: Keep the fix that populates `ipv4`/`ipv6` in SystemInfo before sending.

## Frontend

### Server Map Widget — Download Prompt

When GeoIP is not installed (query `/api/geoip/status`), the widget shows:

```
[Map area - greyed out]
"GeoIP database not installed"
[Download GeoIP Database] button (admin only)
```

- Button calls `POST /api/geoip/download`
- Shows loading spinner during download
- Success toast: "GeoIP database installed. Data will populate as agents reconnect."
- Non-admin users see the message without the button
- Attribution line below map when data is present: "GeoIP by DB-IP"

### Settings Page — GeoIP Management

New route: `/_authed/settings/geoip`

Content:
- Status card: "Installed" / "Not Installed", file size, last updated date
- Download / Update button (shows "Download" when not installed, "Update" when installed)
- Attribution text: "Data provided by DB-IP, licensed under CC BY 4.0"

Same API calls as the widget prompt.

## Code Deletions

- `geoip.rs`: Remove `lookup_online()`, `IpApiResponse` struct
- `agent.rs`: Remove all ip-api.com fallback logic in both SystemInfo and IpChanged handlers
- `GeoIpConfig`: Remove `enabled` field

## Dependencies

- `flate2` (new): gzip decompression via `flate2::read::GzDecoder`
- `reqwest` (existing): HTTP download — `bytes()` method works without `stream` feature for ~1.5 MB
- `maxminddb` (existing): MMDB parsing — DB-IP Lite uses MaxMind-compatible format

## Testing

- Unit test: `GeoIpService::lookup` with a known IP returns expected country code
- Unit test: Status endpoint returns correct installed/not-installed state
- Integration test: Download endpoint (mock HTTP response) → file written → hot-loaded → lookup works
- Frontend test: Widget shows download prompt when no GeoIP data, hides when data present

## File Inventory

### New files
- `crates/server/src/router/api/geoip.rs` — API endpoints
- `apps/web/src/routes/_authed/settings/geoip.tsx` — Settings page

### Modified files
- `crates/server/src/service/geoip.rs` — Remove `lookup_online`, adjust `load` for data directory
- `crates/server/src/state.rs` — `geoip` field type to `Arc<std::sync::RwLock<Option<GeoIpService>>>`, update `new()` with 3-step loading priority
- `crates/server/src/config.rs` — Remove `enabled` from `GeoIpConfig`
- `crates/server/src/router/ws/agent.rs` — Simplify GeoIP call chain, remove online fallback, prefer reported IP over remote_addr
- `crates/server/src/router/mod.rs` — Register geoip API routes
- `crates/server/Cargo.toml` — Add `flate2` dependency
- `apps/web/src/components/dashboard/widgets/server-map.tsx` — Add download prompt + attribution
- `apps/web/src/lib/api-client.ts` or sidebar — Add settings menu item
