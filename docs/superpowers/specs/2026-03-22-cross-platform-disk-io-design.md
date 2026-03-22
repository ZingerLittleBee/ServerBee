# Cross-Platform Disk I/O Collection via sysinfo

**Date:** 2026-03-22
**Status:** Approved

## Problem

The disk I/O collector (`crates/agent/src/collector/disk_io.rs`) only works on Linux, reading `/proc/diskstats` directly. On macOS and Windows, `read_disk_counters()` returns `None`, so the DiskIO widget shows no data.

## Goal

Provide usable disk I/O throughput data on non-Linux platforms. This is explicitly a **per-mount-path** (not per-physical-disk) implementation — sysinfo's public API does not expose stable physical device identifiers (macOS `bsd_name` is private, there is no cross-platform block device name accessor).

## Decision

Keep the Linux `/proc/diskstats` implementation unchanged. Add a sysinfo-based fallback for non-Linux platforms using `Disk::usage()` (available since sysinfo 0.33).

### Why not unify all platforms on sysinfo?

- ServerBee targets Linux VPS. The existing Linux implementation is battle-tested, filters physical disks cleanly (`sda`, `nvme0n1`), and excludes virtual devices.
- sysinfo's Linux IO internally reads `/proc/diskstats` anyway — wrapping it adds abstraction for zero benefit while losing whole-disk vs partition control.
- Minimal change means minimal regression risk.

### Semantic difference: per-disk vs per-mount-path

| | Linux | macOS / Windows |
|---|---|---|
| **Granularity** | Per physical disk (`sda`, `nvme0n1`) | Per mount path (`/`, `C:\`) |
| **DiskIo.name** | Block device name | Mount point path |
| **Dedup** | Physical device filter via `/sys/block` | None (each mount path is unique) |

**Impact on downstream:**
- **Server aggregation** (`record.rs:aggregate_disk_io`): groups by `entry.name`. On non-Linux, this groups by mount path instead of physical disk. Functionally correct, just different granularity.
- **Frontend aggregate chart** (`buildMergedDiskIoSeries`): sums all entries — works regardless of key semantics.
- **Frontend per-disk view** (`disk-io.ts:buildPerDiskSeries`, `disk-io-chart.tsx`): will show mount point paths as series labels instead of device names. Acceptable and arguably more readable for non-Linux users.

## Design

### Change 1: `read_disk_counters()` non-Linux branch

**File:** `crates/agent/src/collector/disk_io.rs`

Replace the `#[cfg(not(target_os = "linux"))]` block (currently returns `None`) with:

```rust
#[cfg(not(target_os = "linux"))]
{
    use sysinfo::{DiskRefreshKind, Disks};

    let disks = Disks::new_with_refreshed_list_specifics(
        DiskRefreshKind::nothing().with_io_usage()
    );
    let mut counters = HashMap::new();

    for disk in disks.list() {
        // Use mount_point as key: stable between samples, unique per mount path.
        // This gives per-mount-path (not per-physical-disk) semantics — see doc above.
        let name = disk.mount_point().to_string_lossy().to_string();

        if counters.contains_key(&name) {
            continue;
        }

        let usage = disk.usage();
        counters.insert(name, DiskCounters {
            read_bytes: usage.total_read_bytes,
            write_bytes: usage.total_written_bytes,
        });
    }

    Some(counters)
}
```

**Key points:**

- `mount_point()` is used as the key on all non-Linux platforms. Mount points are stable across samples (critical for `compute_disk_io`'s key-based delta matching).
- `total_read_bytes` / `total_written_bytes` are cumulative counters since boot, equivalent to Linux sector counts from `/proc/diskstats`.
- Uses `DiskRefreshKind::nothing().with_io_usage()` to only refresh I/O data, skipping unnecessary storage/kind queries.
- Returns `Some(counters)` so `collect()` enters the rate calculation branch.
- Existing `compute_disk_io()` handles rate computation — no changes needed.

### Change 2: Test updates

**File:** `crates/agent/src/collector/disk_io.rs` (internal tests module)

1. Add a deterministic unit test for `compute_disk_io` using mount-path-style keys. This verifies that the rate calculation and key matching work correctly with the key format used on non-Linux:

```rust
#[test]
fn test_compute_disk_io_with_mount_path_keys() {
    let previous = HashMap::from([
        (
            "/".to_string(),
            DiskCounters {
                read_bytes: 1_000_000,
                write_bytes: 2_000_000,
            },
        ),
        (
            "/home".to_string(),
            DiskCounters {
                read_bytes: 500_000,
                write_bytes: 300_000,
            },
        ),
    ]);
    let current = HashMap::from([
        (
            "/".to_string(),
            DiskCounters {
                read_bytes: 1_100_000,
                write_bytes: 2_200_000,
            },
        ),
        (
            "/home".to_string(),
            DiskCounters {
                read_bytes: 600_000,
                write_bytes: 400_000,
            },
        ),
    ]);

    let result = compute_disk_io(&previous, &current, 10.0);

    assert_eq!(result, vec![
        DiskIo {
            name: "/".to_string(),
            read_bytes_per_sec: 10_000,
            write_bytes_per_sec: 20_000,
        },
        DiskIo {
            name: "/home".to_string(),
            read_bytes_per_sec: 10_000,
            write_bytes_per_sec: 10_000,
        },
    ]);
}
```

This is deterministic — it directly exercises `compute_disk_io` with known inputs and asserts exact expected rates. It proves that mount-path keys work identically to device-name keys in the rate calculation. If a key from `previous` is missing in `current` (or vice versa), the test would catch the inflated rate from `unwrap_or_default()`.

**File:** `crates/agent/src/collector/tests.rs`

2. Update the non-Linux first-sample assertion to match Linux precision:

```rust
#[cfg(not(target_os = "linux"))]
assert_eq!(report.disk_io, Some(vec![]));
```

### Change 3: `cfg` guard cleanup

**File:** `crates/agent/src/collector/disk_io.rs`

The top-level `HashSet` import guard can remain as-is:

```rust
#[cfg(any(test, target_os = "linux"))]
use std::collections::HashSet;
```

No `HashSet` is needed in the non-Linux branch since we use `counters.contains_key()` for dedup.

### Change 4: TESTING.md update

**File:** `TESTING.md`

Update test counts and add the new disk I/O test entries to the agent collector section.

## What does NOT change

- `Collector` struct — no new fields
- `disk_io::collect()` — signature unchanged
- `compute_disk_io()` — logic unchanged
- `DiskIo` protocol type in common crate — unchanged
- `Cargo.toml` — sysinfo 0.33 already a dependency
- Server code — functionally unaffected (aggregation works with any string key)
- Frontend code — functionally unaffected (aggregate chart sums all entries; per-disk view shows mount paths as labels)

## Known Limitations

1. **macOS APFS overcounting**: Multiple APFS volumes on the same physical disk (e.g., `/` and `/System/Volumes/Data`) may report overlapping I/O from the same `IOBlockStorageDriver`, inflating the aggregate total.
2. **Windows multi-mount-path overcounting**: A single Windows volume mounted at multiple paths (e.g., `C:\` and a junction) produces duplicate entries with the same underlying I/O counters.
3. **Per-mount-path not per-disk**: Non-Linux `DiskIo.name` contains mount point paths, not physical device names. This is a semantic difference from Linux.
4. **No Windows testing**: sysinfo's Windows disk I/O implementation exists but is untested in this project. If it returns zero counters, the chart shows flat zeros (graceful degradation).

## Scope

~20 lines of production code + ~40 lines of test code across 3 files (disk_io.rs, tests.rs, TESTING.md).
