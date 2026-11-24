# ServerBee Agent (`v1.0.0-beta.7`) Resource Requirements & Performance Evaluation

**Evaluation date:** 2026-11-24  
**Target release:** `serverbee-agent` `v1.0.0-beta.7` (official Linux static musl binaries)  
**Connected server:** Railway-hosted ServerBee Server running `v1.0.0-beta.7` (Southeast Asia / Singapore region, reached over trans-Pacific WAN TLS WebSocket from NYC test VMs)  
**Scope:** End-to-end static footprint, startup/enrollment latency, 5-minute steady-state idle soak, active monitoring/PTY workload, host stress/high-cardinality scaling, offline/reconnect recovery, and systemd cgroup v2 hard-limit sweeps.

---

## 1. Executive summary

`serverbee-agent` (`v1.0.0-beta.7`) ships as a stripped, statically linked Rust musl executable with zero `glibc` or OpenSSL runtime dependencies. Across clean Linux VMs connected to a live `v1.0.0-beta.7` Server over WAN TLS:

- **Static footprint:** `14,008,256 B` (`13.36 MiB`) for `linux-amd64` (`5.57 MiB` gzip-9, `3.67 MiB` xz-6) and `12,684,992 B` (`12.10 MiB`) for `linux-arm64`. Total managed installation footprint (`/opt/serverbee`, `serverbee` CLI, systemd unit, durable state) is **`14,182,379 B` (`~13.53 MiB`)**.
- **Steady-state CPU overhead (3s reporting interval, `CAP_DEFAULT`):** **`0.143%–0.144%`** of 1 vCPU on a 1-vCPU VM (`p95 0.426%`, `max 0.544%`) and **`0.190%`** of 1 vCPU on a 2-vCPU VM (`p95 0.568%`, `max 0.791%`).
- **Steady-state memory (two distinct scopes):**
  - **Agent process (`VmRSS` / `PSS`):** **`11.09–11.18 MiB`**, composed of only **`0.82–0.91 MiB` private heap/stack (`RssAnon`)** plus **`10.27 MiB` reclaimable file-backed executable pages (`RssFile`)**.
  - **Systemd service cgroup (`memory.current`):** **`2.62–2.95 MiB`** under warm page cache (default config, including the `journalctl -f` child spawned by `SecurityManager`), dropping **54% to `1.21 MiB`** when `[security] enabled = false`. After a cold boot / page-cache drop (`drop_caches=3`), cgroup memory including faulted executable pages settles at **`19.0 MiB`** (peak **`21.0 MiB`**) with zero reclaim pressure.
- **Memory stability (300s soak):** Flat plateau with no leak — `RssAnon` moved by **`+12 KiB`** (`+1.89 KiB/min`, 1 vCPU) and **`+8 KiB`** (`+0.65 KiB/min`, 2 vCPU) over 100 reporting cycles.
- **Measured wire bandwidth (`ss -tinp`):** **`158 B/s` TX** (`~475 B` per 3s TLS `SystemReport` frame) and **`0.8 B/s` RX** (30s ping/pong heartbeats) — roughly **`412 MB/month`** (`~13.7 MB/day`), nearly 10× lower than the historical `v0.9.3` `<= 1–2 KB/s` estimate.
- **Hard cgroup v2 floors (1 vCPU, `MemorySwapMax=0`):**
  - **Warm-cache OOM boundary (with 300-process burst):** Survives at **`6 MiB`** (default) / **`4 MiB`** (`security.enabled = false`); OOM-crashes at **`4 MiB`** (default) / **`3 MiB`** (`security.enabled = false`).
  - **Cold-cache zero-thrash floor (`drop_caches=3`, default config):** **`16 MiB`** (`0` reclaim stalls, `0` OOM); **`24 MiB`** eliminates all page-cache pressure (`21.0 MiB` peak). Below `16 MiB` (`12M`–`6M`), the service avoids OOM but continuously evicts and refaults executable code pages (`>137k` reclaim stalls in 20s).
  - **`CPUQuota` floor:** **`0` throttled periods down to `5%`** of 1 vCPU; even at **`2%`**, telemetry remains uninterrupted (`59.02 ms` throttled over 45s).
- **Primary scaling hotspot:** `SysinfoSource` refreshes all processes every 3s via `sysinfo 0.33` solely to compute `process_count`, holding persistent `/proc/<pid>/stat` file descriptors that scale ~1:1 with host process count (`51 → 545` FDs at 519 processes, CPU `3.0×`, cgroup memory `2.3×`). Dense hosts (`>500` processes) should configure `LimitNOFILE=65536`.

---

## 2. Quick-reference hardware & OS requirements

| Resource | Absolute floor (minimal config) | Minimum viable (default config) | Recommended (production / active use) | Evidence & notes |
| --- | --- | --- | --- | --- |
| **CPU** | 1 vCPU (`5%` share / `CPUQuota=5%`) | 1 vCPU (shared / burstable) | 1+ vCPU | Idle uses `0.14%–0.19%` of 1 core; spikes to `0.57%` avg (`3.00%` max) at 519 processes or `0.44%` cgroup (`3.47%` max) during active PTY sessions. |
| **Available RAM (agent budget)** | **16 MiB** (`security.enabled = false`, no swap) | **32 MiB** (cold boot + default `security_events` + headroom) | **64 MiB** (**128 MiB** with Web Terminal shells or `>500` processes) | Cold-cache default peaks at `21.0 MiB` cgroup (`0` pressure at `24 MiB`, `0` stalls at `16 MiB`). 519 host processes add `~4 MiB` cgroup; active PTY shell adds `~2.4 MiB` cgroup. |
| **Host VPS total RAM** | 64–128 MiB Linux VPS (Alpine / minimal Debian) | 128–256 MiB Linux VPS | 512 MiB+ Linux VPS | Agent accounts for `<2%–5%` of a 512 MiB VPS; host kernel and init system dominate total RAM. |
| **Disk space** | **20 MiB** | **30 MiB** | **50–100 MiB** | Binary is `13.36 MiB` (`amd64`) / `12.10 MiB` (`arm64`); total managed install is `13.53 MiB`. Reserve `2×` binary size (`~28 MiB`) for atomic self-upgrade `.bak` rollback plus logs. |
| **Network bandwidth** | **2 Kbps** (~0.45 GB/mo) | **5 Kbps** (~0.5 GB/mo) | **100 Kbps+** (interactive Web Terminal / File Manager) | Measured idle steady-state: `158 B/s` TX + `0.8 B/s` RX (`~412 MB/month`). Periodic active PTY commands measured `971 B/s` TX. |
| **OS & libc** | Linux with `/proc` (no `glibc` required) | Linux (Debian / Ubuntu / RHEL / Alpine / Arch) + systemd or OpenRC | Linux 5.x/6.x with `systemd` (`cgroup v2`) | Official Linux release binaries (`amd64`, `arm64`) are statically linked musl (`not a dynamic executable`). |
| **Architectures** | `linux/amd64`, `linux/arm64` | `linux/amd64`, `linux/arm64` | `linux/amd64`, `linux/arm64` | Prebuilt binaries also ship for `darwin/amd64`, `darwin/arm64`, and `windows/amd64`. |
| **Open file descriptors (`nofile`)** | `>= 256` (small VPS, `<150` procs) | `>= 1024` (default `ulimit -n`) | **`>= 4096`–`65536`** on dense hosts (`>500` procs/threads) | `sysinfo` holds persistent `/proc/<pid>/stat` FDs (~1:1 with host processes; `545` FDs at 519 procs). Set `LimitNOFILE=65536` on dense hosts. |

---

## 3. Test environment & methodology

### 3.1 Provisioned Linux VMs (`boxd`, NYC region)

| VM | Tier | vCPU | RAM / Swap | CPU / Virtualization | OS & Kernel | Init / Cgroup | Docker daemon |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `sb-bench-min` | Minimal / constrained | 1 vCPU | 4 GiB (3.8 GiB usable) / 1.0 GiB swap | AMD EPYC (KVM full virtualization) | Ubuntu 24.04.5 LTS / Linux `6.18.0+` x86_64 | `systemd 255` / `cgroup2fs` | Present (idle) |
| `sb-bench-std` | Standard | 2 vCPU | 8 GiB (7.8 GiB usable) / 1.0 GiB swap | AMD EPYC (KVM full virtualization) | Ubuntu 24.04.5 LTS / Linux `6.18.0+` x86_64 | `systemd 255` / `cgroup2fs` | Present (idle) |

Because `boxd`'s smallest VM size allocates `1 vCPU / 4 GiB`, true low-RAM and low-CPU boundaries were tested on `sb-bench-min` by enforcing **systemd cgroup v2 hard limits** (`MemoryMax=...`, `MemorySwapMax=0`, `CPUQuota=...`) under both warm page cache and cold page cache (`sync && echo 3 > /proc/sys/vm/drop_caches`).

### 3.2 Server & agent runtime configuration

- **Server:** Railway-hosted ServerBee Server running `ghcr.io/zingerlittlebee/serverbee-server:1.0.0-beta.7` (Southeast Asia / Singapore), verified healthy via `/healthz` (`ok`) and `/api/about` (`version: "1.0.0-beta.7"`).
- **Agent installation:** Installed via `deploy/install.sh` (`--method binary --version 1.0.0-beta.7`) into `/opt/serverbee/bin/serverbee-agent`, `/opt/serverbee/etc/agent.toml`, `/usr/local/bin/serverbee`, and `/etc/systemd/system/serverbee-agent.service`.
- **Active runtime settings:**
  - Server-sent `Welcome.report_interval = 3` seconds (in `v1.0.0-beta.7`, the server's `Welcome` frame dictates the active reporting interval).
  - Default capabilities (`CAP_DEFAULT = 3900`): `upgrade`, `ping_icmp`, `ping_tcp`, `ping_http`, `security_events`, `firewall_block`, `ip_quality`, `tailscale`. High-risk capabilities (`terminal`, `exec`, `file`, `docker`) remained disabled except in Scenario 3b (`terminal,exec`).
  - `[security] enabled = true` (default on Linux), which spawns a persistent `journalctl -f --output=json -n 0 ...` child process inside `serverbee-agent.service`.
  - On connect, the server also synced `NetworkProbeSync` (3 targets, 60s interval, 10 packets) and `IpQualitySync` (9 services, 12h interval).

### 3.3 Measurement methodology & accounting scopes

A Python profiler sampled every running scenario at 1-second intervals (`step == 0` excluded from delta-rate statistics):

- **CPU% (1-core basis):** Both whole-window `/proc/<pid>/stat` (`(utime + stime) / CLK_TCK / duration`) and microsecond-resolution cgroup v2 `cpu.stat` (`usage_usec` delta / duration), avoiding 100 Hz tick quantization.
- **Memory scopes (kept separate because they measure different things):**
  - `VmRSS` / `PSS`: Resident memory of `serverbee-agent` alone from `/proc/<pid>/status` and `/proc/<pid>/smaps_rollup`, split into private heap/stack (`RssAnon`) and reclaimable file-backed executable mappings (`RssFile`).
  - `child_rss_kb`: Resident memory of child processes inside `serverbee-agent.service` (`journalctl -f` by default, plus shell children during Web Terminal sessions).
  - `cg_mem_current_kb` / `cg_mem_peak_kb`: Systemd cgroup v2 `memory.current` and `memory.peak` across the agent, its children, kernel slab/stack, and cgroup-charged file cache. On warm-cache starts where the binary was already paged in outside the unit cgroup during installation, cgroup `file` is near zero (`~12 KiB`); cold-cache starts (`drop_caches=3`) charge the faulted binary pages directly into the service cgroup.
- **Exact agent TCP bandwidth:** Read from the agent's established TCP socket via `ss -tinp` (`bytes_acked` for TX, `bytes_received` for RX), excluding SSH and VM control traffic. For reconnect windows, positive per-interval deltas were summed across socket lifecycles.
- **OOM & restart tracking:** Cgroup `memory.events` (`max`, `oom`, `oom_kill`), `cpu.stat` (`nr_throttled`, `throttled_usec`), and `systemctl show -p NRestarts` were recorded on every sweep step so silent restarts or code-page thrashing could not be mistaken for clean operation.

---

## 4. Static footprint & binary characteristics (`v1.0.0-beta.7`)

### 4.1 Release binary sizes & compression

| Artifact (`v1.0.0-beta.7`) | Raw size | `gzip -9` | `zstd -19` | `xz -6` | SHA-256 |
| --- | --- | --- | --- | --- | --- |
| `serverbee-agent-linux-amd64` | **14,008,256 B** (13.36 MiB) | 5,840,959 B (5.57 MiB, 41.7%) | 4,178,828 B (3.99 MiB, 29.8%) | 3,848,612 B (3.67 MiB, 27.5%) | `f3866f4f480d608daf473aa2d872d4e4506a11c4ade95e4343f1b14aee753855` |
| `serverbee-agent-linux-arm64` | **12,684,992 B** (12.10 MiB) | 5,499,235 B (5.24 MiB, 43.4%) | 3,942,297 B (3.76 MiB, 31.1%) | 3,398,760 B (3.24 MiB, 26.8%) | `0ed202ee8aeca57dbe9369f521578830efce804fa6c5326926513eff6eb8953d` |
| `serverbee-agent-darwin-amd64` | 18,662,896 B (17.80 MiB) | — | — | — | Published in release `sha256sums.txt` |
| `serverbee-agent-darwin-arm64` | 17,258,000 B (16.46 MiB) | — | — | — | Published in release `sha256sums.txt` |
| `serverbee-agent-windows-amd64.exe` | 16,436,736 B (15.68 MiB) | — | — | — | Published in release `sha256sums.txt` |

### 4.2 ELF linkage & runtime dependencies (verified on Linux `x86_64`)

- `file`: `ELF 64-bit LSB executable, x86-64, version 1 (SYSV), statically linked, stripped` (equivalent `ARM aarch64` for `linux-arm64`).
- `ldd`: `not a dynamic executable`
- `readelf -d`: `There is no dynamic section in this file.`
- `readelf -l`: 4 `LOAD` segments (`R` 3.33 MiB, `R E` 9.41 MiB, `RW` 0.60 MiB, `RW` 0.03 MiB), `GNU_RELRO`, non-executable `GNU_STACK`.
- **Release toolchain:** Cross-compiled via `cargo zigbuild --release --locked --target {x86_64,aarch64}-unknown-linux-musl` with `ziglang==0.15.2` (pinned in `.github/workflows/release.yml` to preserve musl `mallocng` and avoid Zig 0.16's cross-thread allocator retention issue).
- **TLS & roots:** Embeds `rustls` (`ring`) and `webpki-roots`; neither OpenSSL nor a host CA bundle is required for ServerBee connections.
- **Optional host utilities:** `journalctl` (SSH security log tailing; falls back to `/var/log/auth.log` or `/var/log/secure` when absent), `nft` (for `firewall_block`), and `conntrack` (only when `security.port_scan.enabled = true`, disabled by default).

### 4.3 Installed disk layout (`deploy/install.sh` on `linux-amd64`)

| Path | Size | Purpose |
| --- | --- | --- |
| `/opt/serverbee/bin/serverbee-agent` | `14,008,256 B` | Static agent executable |
| `/opt/serverbee/etc/agent.toml` + `.install-meta` | `375 B` | Agent configuration (`0600`) and installer metadata |
| `/opt/serverbee` (subtotal) | **`14,008,631 B`** (13.36 MiB) | Base install tree |
| `/usr/local/bin/serverbee` | `173,080 B` (169 KiB) | Management CLI script |
| `/etc/systemd/system/serverbee-agent.service` | `517 B` | Systemd unit file |
| `/var/lib/serverbee/capability_grants.events.json` | `151 B` | Durable capability event journal |
| **Total installed footprint** | **`14,182,379 B` (~13.53 MiB)** | Excludes temporary `.bak` copy created during self-upgrade |

---

## 5. Startup & enrollment latency

Measured with `info` logging over a trans-Pacific WAN link (NYC VM → Singapore Server behind Cloudflare).

### 5.1 Fresh enrollment (cold start with one-time `enrollment_code`, `sb-bench-min`)

| Milestone | Elapsed from exec (`T0`) | Delta | Notes |
| --- | --- | --- | --- |
| Process start (`serverbee-agent` entry) | `+1.8 ms` | `1.8 ms` | Static binary load + Tokio runtime init |
| Firewall probe (`nft available`) | `+13.4 ms` | `11.6 ms` | Startup `nft` capability check |
| Enrollment claim confirmed (`POST /api/agent/register`) | `+424.4 ms` | `411.0 ms` | Stage 32-byte run token + HTTPS TLS claim |
| WebSocket connected (`wss://.../api/agent/ws`) | `+1,225.9 ms` | `801.5 ms` | TLS handshake + WebSocket upgrade |
| `Welcome` received (`interval=3s`) | `+1,252.6 ms` | `26.7 ms` | Server admission + welcome frame |
| `Sent SystemInfo` | **`+1,262.1 ms`** | **`9.5 ms`** | `System::new_all()` + concurrent Docker/Tailscale probe |
| Initial subsystem sync (`PingTasksSync` / `NetworkProbeSync` / `IpQualitySync`) | `+1,497.0 ms` | `234.9 ms` | Background subsystem synchronization |

End-to-end fresh enrollment completed in **`1.26 s`** over trans-Pacific WAN, of which only **`~25 ms`** was local agent execution and ~98% was WAN RTT and TLS handshakes.

### 5.2 Warm restart (persisted run token, `sb-bench-std` via `systemctl restart`)

| Milestone | Elapsed from `systemctl restart` | Delta |
| --- | --- | --- |
| Process entry (`starting...`) | `+33.9 ms` | `33.9 ms` (includes systemd stop/start) |
| Firewall probe (`nft available`) | `+51.7 ms` | `17.8 ms` |
| WebSocket connected | `+889.2 ms` | `837.5 ms` (WAN TLS + WS upgrade) |
| `Welcome` received (`interval=3s`) | `+919.2 ms` | `30.0 ms` |
| `Sent SystemInfo` | **`+929.3 ms`** | **`10.1 ms`** |

### 5.3 Startup peak memory

Immediately after `Sent SystemInfo`, `UnlockChecker` executes its initial 9-service IP quality check and `SysinfoSource::new` runs `System::new_all()`, causing a brief initial spike before settling to steady state:

- **`sb-bench-min` (1 vCPU):** `VmHWM = 13,060 KiB` (`12.75 MiB`), warm-cache cgroup `memory.peak = 4,836 KiB` (`4.72 MiB`) → settles to `VmRSS = 11,359 KiB`, cgroup `2,685 KiB`.
- **`sb-bench-std` (2 vCPU):** `VmHWM = 13,092 KiB` (`12.79 MiB`), warm-cache cgroup `memory.peak = 5,440 KiB` (`5.31 MiB`) → settles to `VmRSS = 11,451 KiB`, cgroup `3,019 KiB`.

---

## 6. Steady-state baseline idle soak (300s @ 1s intervals)

Both VMs ran concurrently for 300s (`301` samples each) after ~2 minutes of warmup under default settings (`CAP_DEFAULT`, `security.enabled = true`, `enable_temperature = true`, `report_interval = 3s`).

| Metric | `sb-bench-min` (1 vCPU / 4 GiB) | `sb-bench-std` (2 vCPU / 8 GiB) |
| --- | --- | --- |
| **Agent CPU% (1-core window avg, `/proc`)** | **0.143%** | **0.190%** |
| **Cgroup CPU% (1-core window avg, `cpu.stat`)** | **0.144%** (`p50 0.012%`, `p95 0.426%`, `max 0.544%`) | **0.190%** (`p50 0.015%`, `p95 0.568%`, `max 0.791%`) |
| **Agent `VmRSS` (avg / min–max)** | **11,359 KiB** (11.09 MiB) / `11,344–11,364 KiB` | **11,451 KiB** (11.18 MiB) / `11,444–11,452 KiB` |
| **Agent `RssAnon` (private heap/stack)** | **843 KiB** (0.82 MiB) / `828–848 KiB` | **935 KiB** (0.91 MiB) / `928–936 KiB` |
| **Agent `RssFile` (static binary pages)** | **10,516 KiB** (10.27 MiB, constant) | **10,516 KiB** (10.27 MiB, constant) |
| **Agent `PSS` (avg)** | **11,351 KiB** (11.08 MiB) | **11,443 KiB** (11.18 MiB) |
| **Agent `VmSize` (avg)** | **20,743 KiB** (20.26 MiB) | **24,899 KiB** (24.32 MiB) |
| **Agent `VmHWM` (process lifetime peak)** | `13,060 KiB` (12.75 MiB) | `13,092 KiB` (12.79 MiB) |
| **Child `journalctl -f` (`VmRSS` / `PSS` / `RssAnon`)** | `8,684` / `2,600` / `900 KiB` | `8,564` / `2,519` / `896 KiB` |
| **Systemd cgroup `memory.current` (warm cache)** | **2,685 KiB** (2.62 MiB) / max `2,812 KiB` | **3,019 KiB** (2.95 MiB) / max `3,264 KiB` |
| **Cgroup `anon` / `slab` / `kernel_stack` / `file`** | `1,743` / `535` / `48` / `12 KiB` | `1,831` / `618` / `80` / `12 KiB` |
| **Threads (steady-state)** | **3** (constant) | **5** (constant) |
| **Open file descriptors (avg / range)** | **46.2** (`43–47`) | **51.1** (`48–52`) |
| **TCP socket TX (`ss -tinp` `bytes_acked`)** | **158.4 B/s** (`47,523 B` / 300s) | **158.0 B/s** (`47,407 B` / 300s) |
| **TCP socket RX (`ss -tinp` `bytes_received`)** | **0.8 B/s** (`240 B` / 300s) | **0.8 B/s** (`240 B` / 300s) |
| **`RssAnon` start → end (slope)** | `836 → 848 KiB` (**+12 KiB**, `+1.89 KiB/min`) | `928 → 936 KiB` (**+8 KiB**, `+0.65 KiB/min`) |
| **Server status & persisted records** | `online: true`, records `3 → 8` (`+5` / 5m) | `online: true`, records `3 → 8` (`+5` / 5m) |
| **Restarts / OOM / CPU throttles** | `0` / `0` / `0` | `0` / `0` / `0` |

### Key takeaways

1. **Tokio worker scaling:** `#[tokio::main]` spawns one worker thread per logical CPU, explaining the thread jump (`3 → 5`) and virtual memory increase (`20.26 → 24.32 MiB`) between 1 vCPU and 2 vCPU while adding only **`+92 KiB`** of private `RssAnon`.
2. **Memory stability:** Both VMs remained flat within `8–12 KiB` across 100 consecutive `SystemReport` cycles.
3. **Wire bandwidth:** Each TLS-encrypted `SystemReport` frame averaged **`~475 B`** (`47,523 B / 100` reports), plus 24 B every 30s for WebSocket ping/pong heartbeats (`~158 B/s` TX + `0.8 B/s` RX, or `~412 MB/month`).

---

## 7. Active monitoring load & optional capabilities (60s windows)

| Scenario | VM | Cgroup CPU% (avg / p95 / max) | Agent `VmRSS` / `RssAnon` | Cgroup `mem` (avg / max) | Child `VmRSS` | Threads / FDs | TCP TX / RX |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **Idle baseline (300s ref)** | `std` (2 vCPU) | `0.190%` / `0.568%` / `0.791%` | `11,451` / `935 KiB` | `3,019` / `3,264 KiB` | `8,564 KiB` | `5.0` / `51.1` | `158.0` / `0.8 B/s` |
| **3a. Active Dashboard WS + REST polling** (69 WS frames, 52 REST calls) | `std` (2 vCPU) | `0.187%` / `0.519%` / `0.720%` | `11,452` / `936 KiB` | `3,025` / `3,248 KiB` | `8,564 KiB` | `5.0` / `51.2` | `158.2` / `0.8 B/s` |
| **3b. Active PTY Web Terminal** (`terminal,exec` enabled; 42 chunks, `32.8 KiB` output) | `std` (2 vCPU) | **`0.443%`** / `1.522%` / `3.465%` | `11,458` / `942 KiB` | **`5,471` / `5,980 KiB`** | **`10,371 KiB`** | **`6.0`** / `55.8` | **`971.0` / `31.7 B/s`** |
| **Idle baseline (300s ref, `security=on`)** | `min` (1 vCPU) | `0.144%` / `0.426%` / `0.544%` | `11,359` / `843 KiB` | `2,685` / `2,812 KiB` | `8,684 KiB` | `3.0` / `46.2` | `158.4` / `0.8 B/s` |
| **3c. Security disabled (`[security] enabled = false`)** | `min` (1 vCPU) | `0.146%` / `0.417%` / `0.566%` | `11,282` / `766 KiB` | **`1,239` / `1,364 KiB`** | **`0 KiB`** | `3.0` / `43.0` | `158.8` / `0.8 B/s` |

- **Active dashboard viewing (3a):** Zero measurable agent overhead — the Server fans out already-ingested metrics over `/api/ws/servers` and serves chart queries from SQLite.
- **Interactive PTY terminal (3b):** Adds **`+1` blocking reader thread** (`5 → 6`), **`~4–5` FDs**, **`+2.39 MiB` cgroup memory** (mostly the spawned shell process), and **`~+0.25%` CPU** during periodic command output (`971 B/s` TX). Agent `RssAnon` rose by only **`+7 KiB`**.
- **Disabling `SecurityManager` (3c):** Eliminates the persistent `journalctl -f` subprocess (`8.68 MiB` VmRSS / `2.60 MiB` PSS / `900 KiB` RssAnon), cutting systemd cgroup memory by **54%** (`2.62 MiB → 1.21 MiB`) and freeing ~3 FDs.

---

## 8. Host stress, high-cardinality & offline/reconnect recovery (`sb-bench-std`, 2 vCPU)

| Stress scenario (60s–85s) | Host procs (avg) | Cgroup CPU% (avg / p95 / max) | Agent `VmRSS` / `RssAnon` | Cgroup `mem` (avg / max) | Open FDs (avg) | TCP TX / RX | Telemetry continuity |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **Idle baseline (ref)** | `18` | `0.190%` / `0.568%` / `0.791%` | `11,451` / `935 KiB` | `3,019` / `3,264 KiB` | `51.1` | `158.0` / `0.8 B/s` | Uninterrupted |
| **4a. 100% host CPU saturation** (2 busy workers) | `22` | `0.175%` / `0.547%` / `0.647%` | `11,424` / `908 KiB` | `2,772` / `2,912 KiB` | `55.2` | `158.8` / `0.8 B/s` | Uninterrupted (records `12 → 13`) |
| **4b. High process cardinality** (+500 sleeping procs) | **`519`** | **`0.573%`** / **`1.970%`** / **`3.000%`** | `11,805` / `1,289 KiB` | **`6,961` / `7,144 KiB`** | **`544.7`** | `159.6` / `0.8 B/s` | Uninterrupted |
| **4c. High disk I/O + 200 local TCP sockets** | `20` | `0.216%` / `0.662%` / `0.762%` | `11,821` / `1,305 KiB` | `3,102` / `3,344 KiB` | `54.2` | `158.8` / `0.8 B/s` | Uninterrupted |
| **4d. 40s network blackhole & reconnect (85s)** | `20` | `0.298%` / `0.631%` / `7.271%` | `11,491` / `967 KiB` | `2,815` / `3,816 KiB` | `52.6` | `271.0` / `13,032 B/s`* | Marked offline during block; auto-recovered cleanly |

*\*Note on 4d RX/CPU burst: upon reconnect, the server sends `IpQualitySync` (9 services), triggering an immediate `UnlockChecker` run that accounts for the transient `13.0 KiB/s` avg RX and `7.27%` max CPU burst.*

### Stress analysis

1. **Host CPU saturation (4a):** Pinning both vCPUs at 100% did not interrupt telemetry (`158.8 B/s` TX, `0.175%` agent CPU, no memory growth).
2. **High process cardinality (4b — primary scaling bottleneck):** Adding 500 idle `sleep` processes scaled **open FDs 10.6×** (`51 → 545`, ~1:1 with host processes), **CPU 3.0×** (`0.19% → 0.57%` avg, `3.00%` peak), and **cgroup memory 2.3×** (`2.95 MiB → 6.80 MiB`, peak `6.98 MiB`).
3. **Disk I/O & 200 TCP sockets (4c):** Continuous 16 MiB fsync I/O loops plus 200 active loopback sockets added only `+0.026%` CPU and `+83 KiB` cgroup memory.
4. **Offline & reconnect behavior (4d):** Blocking all packets (`nft drop`) for `40.1 s` caused the server to mark the host `online: false` (`OFFLINE_THRESHOLD_SECS = 30s`). Agent memory and FDs stayed flat (`0` leaks, `0` restarts). Because `connect_and_report` does not enforce an application-level read-idle timeout during a silent packet blackhole, the half-open TCP socket closed `+27.0 s` after unblocking once peer FIN/RST propagated; from disconnect detection to `Sent SystemInfo` took **`1.36 s`** (`0.97 s` jittered 1s backoff + `0.39 s` TLS/WS/`Welcome`/`SystemInfo`).

---

## 9. Minimum viable resource floor sweep (`sb-bench-min`, 1 vCPU, cgroup v2)

### 9.1 Warm-cache `MemoryMax` sweep (`MemorySwapMax=0`, clean restart + 15s steady + 15s 300-proc burst)

| Config mode | `MemoryMax` | Restarts / PID changes | OOM / OOM kills | Cgroup `current` (avg / max) | Cgroup `anon` (avg) | Agent `VmRSS` (avg) | Outcome |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Default (`security=on`) | `64M`–`12M` | `0` / `0` | `0` / `0` | `3,615–3,741` / `4,896–5,108 KiB` | `~1,805 KiB` | `~11,424 KiB` | **PASS** (zero pressure) |
| Default (`security=on`) | `8M` | `0` / `0` | `0` / `0` | `3,663` / `5,088 KiB` | `1,804 KiB` | `11,424 KiB` | **PASS** |
| Default (`security=on`) | **`6M`** | **`0` / `0`** | **`0` / `0`** | **`3,623` / `4,916 KiB`** | **`1,808 KiB`** | **`11,424 KiB`** | **PASS (warm-cache floor)** |
| Default (`security=on`) | **`4M`** | **`4` / `2`** | crash-loop | OOM crash-loop during startup | — | — | **FAIL (OOM)** |
| Minimal (`security=off`) | `16M`–`6M` | `0` / `0` | `0` / `0` | `2,491–2,572` / `3,800–3,960 KiB` | `~875 KiB` | `~11,387 KiB` | **PASS** |
| Minimal (`security=off`) | **`4M`** | **`0` / `0`** | **`0` / `0`** | **`2,537` / `3,892 KiB`** | **`870 KiB`** | **`11,386 KiB`** | **PASS (warm-cache floor)** |
| Minimal (`security=off`) | **`3M`** | **`4` / `2`** | crash-loop | OOM crash-loop during startup | — | — | **FAIL (OOM)** |

### 9.2 Cold-cache `MemoryMax` sweep (`sync && echo 3 > /proc/sys/vm/drop_caches`, default config, `MemorySwapMax=0`)

Dropping the host page cache before service startup forces the `13.36 MiB` static executable (`RssFile`) and `journalctl` shared libraries to charge directly against the service cgroup's `MemoryMax`:

| `MemoryMax` (cold) | Restarts | OOM / kills | Reclaim stalls (`memory.events max`) | Cgroup `current` (avg / max / peak) | Cgroup `anon` / `file` | Agent `VmRSS` / `RssFile` | Outcome |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **`24M`** | `0` | `0` / `0` | **`0`** | **`19,439` / `19,608` / `21,548 KiB`** | `1,713` / `16,560 KiB` | `10,215` / `9,400 KiB` | **PASS — zero reclaim pressure** |
| **`16M`** | `0` | `0` / `0` | **`0`** | **`15,252` / `15,440` / `16,400 KiB`** | `1,706` / `12,419 KiB` | `8,227` / `7,417 KiB` | **PASS — cold-cache zero-stall floor** |
| **`12M`** | `0` | `0` / `0` | **`168,584`** | `12,248` / `12,288` / `12,524 KiB` | `2,757` / `7,926 KiB` | `2,947` / `1,085 KiB` | **DEGRADED — heavy code-page thrashing** |
| `8M` | `0` | `0` / `0` | `169,376` | `8,166` / `8,192` / `8,200 KiB` | `1,804` / `5,707 KiB` | `1,835` / `923 KiB` | **DEGRADED — severe thrashing** |
| `6M` | `0` | `0` / `0` | `137,589` | `6,130` / `6,144` / `6,156 KiB` | `1,250` / `4,530 KiB` | `412` / `58 KiB` | **DEGRADED — severe thrashing** |

### 9.3 `CPUQuota` floor sweep (`sb-bench-min`, 1 vCPU, 45s windows)

| `CPUQuota` | Cgroup CPU% (window avg / p95 / max) | Throttled periods (`nr_throttled`) | Throttled time | TCP TX (avg) | Server records | Outcome |
| --- | --- | --- | --- | --- | --- | --- |
| `25%` | `0.117%` / `0.372%` / `0.406%` | **`0`** | `0.00 ms` | `151.7 B/s` | `23 → 24` | **PASS (zero throttling)** |
| `10%` | `0.144%` / `0.484%` / `0.512%` | **`0`** | `0.00 ms` | `163.5 B/s` | `24 → 25` | **PASS (zero throttling)** |
| **`5%`** | `0.130%` / `0.391%` / `0.467%` | **`0`** | **`0.00 ms`** | `157.5 B/s` | `25 → 26` | **PASS (zero-throttle CPU floor)** |
| `2%` | `0.141%` / `0.376%` / `0.458%` | `15` | `59.02 ms` | `157.5 B/s` | `26 → 27` | **PASS (minor burst throttling)** |

---

## 10. Engineering bottlenecks & practical tuning guidance

### 10.1 Bottlenecks identified in `v1.0.0-beta.7`

1. **Persistent `/proc/<pid>/stat` FD scaling in `SysinfoSource` (high impact on dense hosts):**
   - `crates/agent/src/collector/source.rs` calls `sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing())` every 3s solely to compute `process_count = sys.processes().len()`.
   - `sysinfo 0.33` keeps open FDs for `/proc/<pid>/stat`, causing open FDs to scale ~1:1 with host process count (`51 → 545` FDs at 519 processes), tripling CPU (`0.19% → 0.57%`) and more than doubling cgroup memory (`2.95 → 6.80 MiB`).
   - Unlike `serverbee-server.service` (which sets `LimitNOFILE=65536`), `serverbee-agent.service` in `deploy/install.sh` currently omits `LimitNOFILE`. Hosts approaching `950–1,000+` processes/threads can hit the default `1024` FD cap.
   - *Future optimization opportunity:* Counting numeric PID entries directly in `/proc` on Linux would remove persistent `/proc` FDs and eliminate the `O(N)` per-tick FD and slab overhead.
2. **`journalctl -f` subprocess overhead on ultra-low-RAM VPS:**
   - Default `[security] enabled = true` spawns a persistent `journalctl -f --output=json` child (`~8.6 MiB` VmRSS / `~2.5 MiB` PSS / `~900 KiB` private anon), accounting for **~54% of steady-state warm-cache cgroup memory** (`2.62 → 1.21 MiB` when disabled).
3. **Server-dictated reporting cadence:**
   - The server sends `Welcome { report_interval: 3 }` (`crates/server/src/router/ws/agent/mod.rs`) and the agent reporting loop honors that server-supplied interval (`crates/agent/src/reporter/mod.rs`). Changing `[collector] interval` in `agent.toml` does not alter the WebSocket reporting interval in `v1.0.0-beta.7`.
4. **Silent packet-blackhole reconnect delay:**
   - Under a silent `DROP` firewall blackhole with no TCP RST, the agent waits on TCP retransmission / peer close (`+27.0 s` after network restoration in test 4d before reconnecting in `1.36 s`).

### 10.2 Practical deployment & tuning recommendations

- **Low-RAM VPS (`<= 256 MiB` total RAM):**
  - Install the static binary (`deploy/install.sh` binary mode) instead of Docker to avoid container runtime overhead.
  - If SSH journal monitoring is unnecessary, disable it in `/opt/serverbee/etc/agent.toml` (`[security] enabled = false`) to cut steady-state cgroup memory by ~54% (`1.21 MiB` warm / `<16 MiB` cold).
  - Keep high-risk capabilities (`terminal`, `exec`, `file`, `docker`) disabled unless required.
  - Recommended systemd guardrails for constrained VPS hosts:
    ```ini
    [Service]
    MemoryMax=64M
    CPUQuota=15%
    ```
    Use `MemoryMax=32M` only when `terminal`/`exec` are disabled and host process counts stay below `300`. Avoid setting `MemoryMax` below `24M` with default config (`<16M` triggers heavy cold-boot executable page-cache thrashing).
- **Dense production hosts (`>500` processes / containers):**
  - Add a systemd drop-in raising the open-file limit (`LimitNOFILE=65536`) and allocate `MemoryMax=128M` (or leave memory uncapped) so `sysinfo`'s `/proc` descriptors and interactive Web Terminal shells have ample headroom.

---

## 11. Reproduction outline

To reproduce these measurements on a fresh Linux host with `systemd` (`cgroup v2`):

1. Provision a 1-vCPU and 2-vCPU Linux VM (Ubuntu 24.04 or equivalent `systemd` / `cgroup2fs` host) and connect to a ServerBee Server running the same target release (`v1.0.0-beta.7`).
2. Install the official static binary with `deploy/install.sh` (`--method binary --version 1.0.0-beta.7`) and verify linkage (`file`, `ldd`, `readelf -d`) and `/opt/serverbee` disk footprint.
3. Sample at 1s intervals for 300s (after warmup), reading:
   - `/proc/<pid>/stat`, `/proc/<pid>/status` (`VmRSS`, `RssAnon`, `RssFile`, `VmSize`, `VmHWM`, `Threads`), `/proc/<pid>/smaps_rollup` (`Pss`), and `/proc/<pid>/fd` count;
   - child process RSS/PSS under `/sys/fs/cgroup/system.slice/serverbee-agent.service/cgroup.procs`;
   - cgroup v2 `cpu.stat`, `memory.current`, `memory.peak`, `memory.stat`, and `memory.events`;
   - exact agent socket counters via `ss -tinp` (`bytes_acked`, `bytes_received`).
4. Exercise active dashboard WS/REST clients, an active PTY session (`--allow-cap terminal --allow-cap exec`), `[security] enabled = false`, 100% host CPU saturation, 500 sleeping processes, disk I/O + 200 local TCP sockets, and a 40s `nft` packet drop + unblock cycle.
5. Run systemd drop-in sweeps on the 1-vCPU VM with `MemorySwapMax=0` and `MemoryMax` from `64M` down to `3M` (both warm cache and after `sync && echo 3 > /proc/sys/vm/drop_caches`), plus `CPUQuota` from `25%` down to `2%`, verifying `memory.events`, `cpu.stat`, `NRestarts`, and live record arrival on the Server after each step.
