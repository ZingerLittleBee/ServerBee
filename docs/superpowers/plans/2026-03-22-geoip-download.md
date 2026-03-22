# GeoIP Download Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable one-click download of a GeoIP country database from the UI so the Server Map widget shows server locations without manual configuration.

**Architecture:** Server gets two new API endpoints (`GET /api/geoip/status`, `POST /api/geoip/download`) that manage a DB-IP Lite Country MMDB file in the data directory. The `GeoIpService` is wrapped in `Arc<RwLock>` for hot-reload after download. Agent-side SystemInfo already carries `ipv4`/`ipv6`. Frontend adds a download prompt in the Server Map widget and a GeoIP settings page.

**Tech Stack:** Rust (Axum, maxminddb, reqwest, flate2), React (TanStack Query, shadcn/ui)

**Spec:** `docs/superpowers/specs/2026-03-22-geoip-download-design.md`

---

### Task 1: Backend — GeoIpService refactor and config cleanup

**Files:**
- Modify: `crates/server/Cargo.toml` (add `flate2`)
- Modify: `crates/server/src/config.rs:217-222` (remove `enabled` from `GeoIpConfig`)
- Modify: `crates/server/src/service/geoip.rs` (remove `lookup_online`, remove inner `Arc`, add `load_from_bytes`, add `download_and_load`)
- Modify: `crates/server/src/state.rs:34,103-107,128` (change `geoip` to `Arc<RwLock>`, add `geoip_downloading` AtomicBool, update startup logic)

- [ ] **Step 1: Add flate2 dependency**

In `crates/server/Cargo.toml`, add under `[dependencies]`:
```toml
flate2 = "1"
```

- [ ] **Step 2: Remove `enabled` from GeoIpConfig**

In `crates/server/src/config.rs`, change `GeoIpConfig` (line 217-222) from:
```rust
pub struct GeoIpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mmdb_path: String,
}
```
to:
```rust
pub struct GeoIpConfig {
    #[serde(default)]
    pub mmdb_path: String,
}
```

- [ ] **Step 3: Rewrite geoip.rs**

Replace the entire `crates/server/src/service/geoip.rs` with:
```rust
use std::io::Read;
use std::net::IpAddr;
use std::path::Path;

use chrono::Datelike;
use maxminddb::Reader;
use serde::Deserialize;

/// GeoIP lookup result
pub struct GeoLookup {
    pub country_code: Option<String>,
    pub region: Option<String>,
}

/// Thread-safe GeoIP reader backed by MaxMind MMDB.
pub struct GeoIpService {
    reader: Reader<Vec<u8>>,
    /// Which file this was loaded from (for status endpoint).
    pub source_path: String,
}

#[derive(Deserialize)]
struct GeoCity {
    country: Option<GeoCountry>,
    subdivisions: Option<Vec<GeoSubdivision>>,
    city: Option<GeoCityNames>,
}

#[derive(Deserialize)]
struct GeoCountry {
    iso_code: Option<String>,
}

#[derive(Deserialize)]
struct GeoSubdivision {
    names: Option<std::collections::BTreeMap<String, String>>,
}

#[derive(Deserialize)]
struct GeoCityNames {
    names: Option<std::collections::BTreeMap<String, String>>,
}

/// Default filename for the downloaded DB-IP Lite Country database.
pub const DBIP_FILENAME: &str = "dbip-country-lite.mmdb";

impl GeoIpService {
    /// Load from a file path. Returns None if file doesn't exist or is invalid.
    pub fn load(mmdb_path: &str) -> Option<Self> {
        if mmdb_path.is_empty() || !Path::new(mmdb_path).exists() {
            return None;
        }

        match Reader::open_readfile(mmdb_path) {
            Ok(reader) => {
                tracing::info!("GeoIP MMDB loaded from {mmdb_path}");
                Some(Self {
                    reader,
                    source_path: mmdb_path.to_string(),
                })
            }
            Err(e) => {
                tracing::error!("Failed to load GeoIP MMDB from {mmdb_path}: {e}");
                None
            }
        }
    }

    /// Load from in-memory bytes (used after download + decompress).
    pub fn load_from_bytes(bytes: Vec<u8>, source_path: String) -> Result<Self, String> {
        Reader::from_source(bytes)
            .map(|reader| Self { reader, source_path })
            .map_err(|e| format!("Invalid MMDB data: {e}"))
    }

    /// Lookup an IP address and return country/region info.
    pub fn lookup(&self, ip: IpAddr) -> GeoLookup {
        if ip.is_loopback() || is_private(&ip) {
            return GeoLookup {
                country_code: None,
                region: None,
            };
        }

        match self.reader.lookup::<GeoCity>(ip) {
            Ok(city) => {
                let country_code = city.country.and_then(|c| c.iso_code);
                let region = city
                    .city
                    .and_then(|c| c.names)
                    .and_then(|n| n.get("en").cloned())
                    .or_else(|| {
                        city.subdivisions
                            .and_then(|subs| subs.into_iter().next())
                            .and_then(|s| s.names)
                            .and_then(|n| n.get("en").cloned())
                    });
                GeoLookup {
                    country_code,
                    region,
                }
            }
            Err(maxminddb::MaxMindDBError::AddressNotFoundError(_)) => GeoLookup {
                country_code: None,
                region: None,
            },
            Err(e) => {
                tracing::debug!("GeoIP lookup failed for {ip}: {e}");
                GeoLookup {
                    country_code: None,
                    region: None,
                }
            }
        }
    }
}

/// Download DB-IP Lite Country MMDB, decompress, save to data_dir, return loaded service.
pub async fn download_dbip(data_dir: &str) -> Result<GeoIpService, String> {
    let now = chrono::Utc::now();
    let url = format!(
        "https://download.db-ip.com/free/dbip-country-lite-{}-{:02}.mmdb.gz",
        now.year(),
        now.month()
    );
    tracing::info!("Downloading GeoIP database from {url}");

    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to download: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to download: server returned {}", resp.status()));
    }

    let compressed = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response: {e}"))?;

    // Decompress gzip
    let mut decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(&compressed));
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .map_err(|e| format!("Failed to decompress: {e}"))?;

    // Validate it's a valid MMDB before writing to disk
    let final_path = Path::new(data_dir).join(DBIP_FILENAME);
    let service = GeoIpService::load_from_bytes(decompressed.clone(), final_path.display().to_string())?;

    // Atomic write: tmp file then rename
    std::fs::create_dir_all(data_dir)
        .map_err(|e| format!("Failed to create data directory: {e}"))?;
    let tmp_path = Path::new(data_dir).join(format!("{DBIP_FILENAME}.tmp"));
    std::fs::write(&tmp_path, &decompressed)
        .map_err(|e| format!("Failed to write database: {e}"))?;
    std::fs::rename(&tmp_path, &final_path)
        .map_err(|e| format!("Failed to save database: {e}"))?;

    tracing::info!("GeoIP database saved to {}", final_path.display());
    Ok(service)
}

fn is_private(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(_) => false,
    }
}
```

- [ ] **Step 4: Update AppState**

In `crates/server/src/state.rs`:

a) Add import at top:
```rust
use std::sync::atomic::AtomicBool;
```

b) Change `geoip` field (line 34) from:
```rust
pub geoip: Option<GeoIpService>,
```
to:
```rust
pub geoip: Arc<std::sync::RwLock<Option<GeoIpService>>>,
pub geoip_downloading: AtomicBool,
```

c) Replace startup logic (lines 103-107) from:
```rust
let geoip = if config.geoip.enabled {
    GeoIpService::load(&config.geoip.mmdb_path)
} else {
    None
};
```
to:
```rust
let geoip = if !config.geoip.mmdb_path.is_empty() {
    GeoIpService::load(&config.geoip.mmdb_path)
} else {
    let default_path = std::path::Path::new(&config.server.data_dir)
        .join(crate::service::geoip::DBIP_FILENAME);
    GeoIpService::load(&default_path.display().to_string())
};
if geoip.is_some() {
    tracing::info!("GeoIP database loaded");
} else {
    tracing::info!("GeoIP database not available — download via Settings or Server Map widget");
}
```

d) Update struct initialization (line 128) from:
```rust
geoip,
```
to:
```rust
geoip: Arc::new(std::sync::RwLock::new(geoip)),
geoip_downloading: AtomicBool::new(false),
```

- [ ] **Step 5: Skip build verification**

Build will fail because `agent.rs` still references `state.geoip` as `Option`. This is expected — Task 2 fixes those references.

- [ ] **Step 6: Commit**

```
git add crates/server/Cargo.toml crates/server/src/config.rs crates/server/src/service/geoip.rs crates/server/src/state.rs
git commit -m "refactor: rewrite GeoIpService with RwLock hot-reload and download support"
```

---

### Task 2: Backend — Simplify agent.rs GeoIP call chain and fix stale data

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs:228-261,781-809,998-1015` (remove online fallback, prefer reported IP, unconditional geo write)
- Modify: `crates/server/src/service/server.rs:201-206` (remove is_some guards)

- [ ] **Step 1: Simplify SystemInfo handler**

In `crates/server/src/router/ws/agent.rs`, replace the GeoIP block in SystemInfo handler (lines 229-261) with:
```rust
            // Resolve GeoIP — prefer agent-reported public IP, fall back to remote_addr
            let ip = info
                .ipv4
                .as_deref()
                .or(info.ipv6.as_deref())
                .and_then(|ip| ip.parse::<std::net::IpAddr>().ok())
                .or_else(|| {
                    state
                        .agent_manager
                        .get_remote_addr(server_id)
                        .map(|addr| addr.ip())
                });

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

- [ ] **Step 2: Simplify IpChanged handler**

In the IpChanged handler (around lines 781-809), replace the GeoIP re-lookup block with:
```rust
                        // Re-run GeoIP lookup based on the new IPs
                        let ip_to_lookup = ipv4
                            .as_deref()
                            .or(ipv6.as_deref())
                            .and_then(|ip| ip.parse::<std::net::IpAddr>().ok());
                        if let Some(ip) = ip_to_lookup {
                            let geo = {
                                let guard = state.geoip.read().unwrap();
                                guard.as_ref().map(|g| g.lookup(ip))
                            };
                            if let Some(geo) = geo {
                                if let Err(e) = update_server_geo(
                                    &state.db,
                                    server_id,
                                    geo.region,
                                    geo.country_code,
                                )
                                .await
                                {
                                    tracing::error!(
                                        "Failed to update GeoIP for {server_id}: {e}"
                                    );
                                }
                            }
                        }
```

Note: The `update_server_geo` call is now unconditional — the old `if geo.region.is_some() || geo.country_code.is_some()` guard is removed.

- [ ] **Step 3: Fix stale data in update_system_info**

In `crates/server/src/service/server.rs`, replace lines 201-206:
```rust
        if region.is_some() {
            active.region = Set(region);
        }
        if country_code.is_some() {
            active.country_code = Set(country_code);
        }
```
with:
```rust
        active.region = Set(region);
        active.country_code = Set(country_code);
```

- [ ] **Step 4: Build and test**

Run: `cargo build --workspace 2>&1 | tail -5`
Expected: Build succeeds

Run: `cargo test --workspace 2>&1 | tail -10`
Expected: All tests pass

- [ ] **Step 5: Commit**

```
git add crates/server/src/router/ws/agent.rs crates/server/src/service/server.rs
git commit -m "fix: simplify GeoIP call chain, prefer reported IP, fix stale geo data"
```

---

### Task 3: Backend — GeoIP API endpoints

**Files:**
- Create: `crates/server/src/router/api/geoip.rs`
- Modify: `crates/server/src/router/api/mod.rs:1-25,60-81` (add module + route registration)

- [ ] **Step 1: Create geoip.rs API module**

Create `crates/server/src/router/api/geoip.rs`:
```rust
use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::State;
use axum::{Json, Router, routing};
use serde::Serialize;

use crate::service::geoip;
use crate::state::AppState;

#[derive(Serialize)]
struct GeoIpStatus {
    installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<String>,
}

#[derive(Serialize)]
struct DownloadResponse {
    success: bool,
    message: String,
}

async fn geoip_status(State(state): State<Arc<AppState>>) -> Json<crate::error::ApiResponse<GeoIpStatus>> {
    let guard = state.geoip.read().unwrap();
    let status = match guard.as_ref() {
        Some(service) => {
            let source = if !state.config.geoip.mmdb_path.is_empty() {
                "custom"
            } else {
                "downloaded"
            };
            let (file_size, updated_at) = std::fs::metadata(&service.source_path)
                .map(|m| {
                    let size = m.len() as i64;
                    let modified = m.modified().ok().map(|t| {
                        let dt: chrono::DateTime<chrono::Utc> = t.into();
                        dt.to_rfc3339()
                    });
                    (Some(size), modified)
                })
                .unwrap_or((None, None));
            GeoIpStatus {
                installed: true,
                source: Some(source.to_string()),
                file_size,
                updated_at,
            }
        }
        None => GeoIpStatus {
            installed: false,
            source: None,
            file_size: None,
            updated_at: None,
        },
    };
    Json(crate::error::ApiResponse { data: status })
}

async fn geoip_download(State(state): State<Arc<AppState>>) -> Json<crate::error::ApiResponse<DownloadResponse>> {
    // Concurrent download guard
    if state.geoip_downloading.swap(true, Ordering::SeqCst) {
        return Json(crate::error::ApiResponse {
            data: DownloadResponse {
                success: false,
                message: "Download already in progress".to_string(),
            },
        });
    }

    let result = geoip::download_dbip(&state.config.server.data_dir).await;

    match result {
        Ok(service) => {
            let mut guard = state.geoip.write().unwrap();
            *guard = Some(service);
            state.geoip_downloading.store(false, Ordering::SeqCst);
            Json(crate::error::ApiResponse {
                data: DownloadResponse {
                    success: true,
                    message: "GeoIP database installed successfully".to_string(),
                },
            })
        }
        Err(e) => {
            state.geoip_downloading.store(false, Ordering::SeqCst);
            Json(crate::error::ApiResponse {
                data: DownloadResponse {
                    success: false,
                    message: e,
                },
            })
        }
    }
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/geoip/status", routing::get(geoip_status))
        .route("/geoip/download", routing::post(geoip_download))
}
```

- [ ] **Step 2: Register in router mod**

In `crates/server/src/router/api/mod.rs`:

a) Add module declaration (alphabetical order, after `file`):
```rust
pub mod geoip;
```

b) Add to admin-only routes (inside the `require_admin` layer block, after `.merge(incident::router())`):
```rust
                        .merge(geoip::router())
```

- [ ] **Step 3: Build and test**

Run: `cargo build --workspace 2>&1 | tail -5`
Expected: Build succeeds

Run: `cargo clippy --workspace -- -D warnings 2>&1 | tail -5`
Expected: No warnings

- [ ] **Step 4: Commit**

```
git add crates/server/src/router/api/geoip.rs crates/server/src/router/api/mod.rs
git commit -m "feat: add GeoIP status and download API endpoints"
```

---

### Task 4: Frontend — Server Map widget download prompt

**Files:**
- Modify: `apps/web/src/components/dashboard/widgets/server-map.tsx`

- [ ] **Step 1: Add download prompt to Server Map widget**

In `apps/web/src/components/dashboard/widgets/server-map.tsx`, replace the no-data message (line 129-131):
```tsx
      {countryGroups.length === 0 && (
        <p className="py-2 text-center text-muted-foreground text-xs">No server location data available</p>
      )}
```
with a GeoIP status check and download prompt. The component needs to:
- Query `GET /api/geoip/status` to check if GeoIP is installed
- If not installed: show "GeoIP database not installed" + Download button (admin only)
- If installed but no data: show "No server location data available"
- After successful download: show success toast
- Add attribution line "GeoIP by DB-IP" when map has data

Add imports at top:
```tsx
import { useMutation, useQuery } from '@tanstack/react-query'
import { Download } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { useAuth } from '@/hooks/use-auth'
import { api } from '@/lib/api-client'
```

Add inside the component before the return:
```tsx
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  const { data: geoStatus } = useQuery<{ installed: boolean; source?: string }>({
    queryKey: ['geoip-status'],
    queryFn: () => api.get('/api/geoip/status')
  })

  const downloadMutation = useMutation({
    mutationFn: () => api.post<{ success: boolean; message: string }>('/api/geoip/download'),
    onSuccess: (data) => {
      if (data.success) {
        toast.success(data.message)
      } else {
        toast.error(data.message)
      }
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Download failed')
    }
  })
```

Replace the no-data section at bottom of return with:
```tsx
      {countryGroups.length === 0 && (
        geoStatus?.installed === false ? (
          <div className="space-y-2 py-2 text-center">
            <p className="text-muted-foreground text-xs">GeoIP database not installed</p>
            {isAdmin && (
              <Button
                disabled={downloadMutation.isPending}
                onClick={() => downloadMutation.mutate()}
                size="sm"
                variant="outline"
              >
                <Download className="mr-1 size-3.5" />
                {downloadMutation.isPending ? 'Downloading...' : 'Download GeoIP Database'}
              </Button>
            )}
          </div>
        ) : (
          <p className="py-2 text-center text-muted-foreground text-xs">No server location data available</p>
        )
      )}
      {countryGroups.length > 0 && (
        <p className="text-right text-muted-foreground text-[10px]">GeoIP by DB-IP</p>
      )}
```

- [ ] **Step 2: Verify lint and typecheck**

Run: `bun x ultracite check 2>&1 | tail -10`
Run: `cd apps/web && bun run typecheck 2>&1 | tail -5`
Expected: No new errors

- [ ] **Step 3: Commit**

```
git add apps/web/src/components/dashboard/widgets/server-map.tsx
git commit -m "feat: add GeoIP download prompt to Server Map widget"
```

---

### Task 5: Frontend — GeoIP Settings page

**Files:**
- Create: `apps/web/src/routes/_authed/settings/geoip.tsx`
- Modify: `apps/web/src/components/app-sidebar.tsx:57-72` (add nav item)

- [ ] **Step 1: Create GeoIP settings page**

Create `apps/web/src/routes/_authed/settings/geoip.tsx`:
```tsx
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Database, Download, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'

export const Route = createFileRoute('/_authed/settings/geoip')({
  component: GeoIpPage
})

interface GeoIpStatus {
  file_size?: number
  installed: boolean
  source?: string
  updated_at?: string
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

function GeoIpPage() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()

  const { data: status, isLoading } = useQuery<GeoIpStatus>({
    queryKey: ['geoip-status'],
    queryFn: () => api.get<GeoIpStatus>('/api/geoip/status')
  })

  const downloadMutation = useMutation({
    mutationFn: () => api.post<{ success: boolean; message: string }>('/api/geoip/download'),
    onSuccess: (data) => {
      if (data.success) {
        toast.success(data.message)
        queryClient.invalidateQueries({ queryKey: ['geoip-status'] })
      } else {
        toast.error(data.message)
      }
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Download failed')
    }
  })

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">GeoIP Database</h1>

      <div className="max-w-2xl space-y-6">
        <div className="rounded-lg border bg-card p-6">
          {isLoading ? (
            <div className="space-y-3">
              <Skeleton className="h-5 w-32" />
              <Skeleton className="h-4 w-48" />
            </div>
          ) : (
            <div className="space-y-4">
              <div className="flex items-center gap-3">
                <Database className="size-5 text-muted-foreground" />
                <div>
                  <p className="font-medium">
                    {status?.installed ? 'Installed' : 'Not Installed'}
                  </p>
                  {status?.installed && status.source === 'custom' && (
                    <p className="text-muted-foreground text-sm">Using custom MMDB file</p>
                  )}
                  {status?.installed && status.file_size && (
                    <p className="text-muted-foreground text-sm">
                      {formatBytes(status.file_size)}
                      {status.updated_at && ` · Updated ${new Date(status.updated_at).toLocaleDateString()}`}
                    </p>
                  )}
                  {!status?.installed && (
                    <p className="text-muted-foreground text-sm">
                      Download the GeoIP database to show server locations on the map
                    </p>
                  )}
                </div>
              </div>

              {status?.source !== 'custom' && (
                <Button
                  disabled={downloadMutation.isPending}
                  onClick={() => downloadMutation.mutate()}
                  variant="outline"
                >
                  {status?.installed ? (
                    <RefreshCw className={`mr-1.5 size-4 ${downloadMutation.isPending ? 'animate-spin' : ''}`} />
                  ) : (
                    <Download className="mr-1.5 size-4" />
                  )}
                  {downloadMutation.isPending
                    ? 'Downloading...'
                    : status?.installed
                      ? 'Update'
                      : 'Download'}
                </Button>
              )}
            </div>
          )}
        </div>

        <p className="text-muted-foreground text-xs">
          Data provided by{' '}
          <a className="underline" href="https://db-ip.com" rel="noopener noreferrer" target="_blank">
            DB-IP
          </a>
          , licensed under{' '}
          <a
            className="underline"
            href="https://creativecommons.org/licenses/by/4.0/"
            rel="noopener noreferrer"
            target="_blank"
          >
            CC BY 4.0
          </a>
        </p>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Add sidebar navigation item**

In `apps/web/src/components/app-sidebar.tsx`, add to `settingsItems` array (after the `audit-logs` entry, before the final `settings` entry):
```tsx
  { to: '/settings/geoip', labelKey: 'nav_geoip', icon: MapPin, adminOnly: true },
```

Add `MapPin` to the lucide-react import at the top of the file (don't use `Globe` — it's already used by network-probes).

- [ ] **Step 3: Add i18n keys**

Add `"nav_geoip": "GeoIP"` to both:
- `apps/web/src/locales/en/common.json`
- `apps/web/src/locales/zh/common.json`

- [ ] **Step 4: Verify lint and typecheck**

Run: `bun x ultracite check 2>&1 | tail -10`
Run: `cd apps/web && bun run typecheck 2>&1 | tail -5`
Expected: No new errors

- [ ] **Step 5: Commit**

```
git add apps/web/src/routes/_authed/settings/geoip.tsx apps/web/src/components/app-sidebar.tsx
git commit -m "feat: add GeoIP settings page with download/update button"
```

---

### Task 6: Final verification and cleanup

**Files:**
- All modified files from Tasks 1-5

- [ ] **Step 1: Full backend build + lint + test**

Run: `cargo build --workspace 2>&1 | tail -5`
Run: `cargo clippy --workspace -- -D warnings 2>&1 | tail -5`
Run: `cargo test --workspace 2>&1 | tail -10`
Expected: All pass

- [ ] **Step 2: Full frontend build + lint + test**

Run: `bun x ultracite check 2>&1 | tail -10`
Run: `cd apps/web && bun run typecheck 2>&1 | tail -5`
Run: `bun run test -- --run 2>&1 | tail -10`
Expected: All pass

- [ ] **Step 3: Verify no ip-api.com references remain**

Run: `grep -r "ip-api\|ip_api\|IpApiResponse\|lookup_online" crates/ --include="*.rs"`
Expected: No matches

- [ ] **Step 4: Verify enabled field is removed**

Run: `grep -r "geoip.*enabled\|enabled.*geoip" crates/ --include="*.rs"`
Expected: No matches (other than possible comments)

- [ ] **Step 5: Commit any fixups**

If any fixes were needed, commit them:
```
git add -A
git commit -m "chore: final cleanup for GeoIP download feature"
```
