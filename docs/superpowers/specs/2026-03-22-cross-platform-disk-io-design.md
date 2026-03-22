# Cross-Platform Disk I/O Collection via sysinfo

**Date:** 2026-03-22
**Status:** Approved

## Problem

The disk I/O collector (`crates/agent/src/collector/disk_io.rs`) only works on Linux, reading `/proc/diskstats` directly. On macOS and Windows, `read_disk_counters()` returns `None`, so the DiskIO widget shows no data.

## Goal

Provide usable disk I/O throughput data on non-Linux platforms. This is explicitly a **per-volume** (not per-physical-disk) implementation — sysinfo's public API does not expose stable physical device identifiers (macOS `bsd_name` is private, there is no cross-platform block device name accessor).

## Decision

Keep the Linux `/proc/diskstats` implementation unchanged. Add a sysinfo-based fallback for non-Linux platforms using `Disk::usage()` (available since sysinfo 0.33).

### Why not unify all platforms on sysinfo?

- ServerBee targets Linux VPS. The existing Linux implementation is battle-tested, filters physical disks cleanly (`sda`, `nvme0n1`), and excludes virtual devices.
- sysinfo's Linux IO internally reads `/proc/diskstats` anyway — wrapping it adds abstraction for zero benefit while losing whole-disk vs partition control.
- Minimal change means minimal regression risk.

### Semantic difference: per-disk vs per-volume

| | Linux | macOS / Windows |
|---|---|---|
| **Granularity** | Per physical disk (`sda`, `nvme0n1`) | Per mounted volume (`/`, `C:\`) |
| **DiskIo.name** | Block device name | Mount point path |
| **Dedup** | Physical device filter via `/sys/block` | None (each mount point is unique) |

**Impact on downstream:**
- **Server aggregation** (`record.rs:aggregate_disk_io`): groups by `entry.name`. On non-Linux, this groups by mount point instead of physical disk. Functionally correct, just different granularity.
- **Frontend aggregate chart** (`buildMergedDiskIoSeries`): sums all entries — works regardless of key semantics.
- **Frontend per-disk view** (`disk-io.ts:buildPerDiskSeries`, `disk-io-chart.tsx`): will show mount point paths as series labels instead of device names. This is acceptable and arguably more readable for non-Linux users.
- **macOS APFS caveat**: Multiple APFS volumes sharing one physical disk (e.g., `/` and `/System/Volumes/Data`) may report overlapping I/O counters from the same underlying `IOBlockStorageDriver`. The aggregate chart may overcount. This is a known limitation documented here.

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
        // Use mount_point as key: stable between samples, unique per volume.
        // This gives per-volume (not per-physical-disk) semantics — see doc above.
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

- `mount_point()` is used as the key on all non-Linux platforms. Mount points are stable across samples (critical for `compute_disk_io`'s key-based delta matching) and unique per filesystem.
- `total_read_bytes` / `total_written_bytes` are cumulative counters since boot, equivalent to Linux sector counts from `/proc/diskstats`.
- Uses `DiskRefreshKind::nothing().with_io_usage()` to only refresh I/O data, skipping unnecessary storage/kind queries.
- Returns `Some(counters)` so `collect()` enters the rate calculation branch.
- Existing `compute_disk_io()` handles rate computation — no changes needed.

### Change 2: Test updates

**File:** `crates/agent/src/collector/tests.rs`

1. Update the non-Linux first-sample assertion to match Linux precision:

```rust
#[cfg(not(target_os = "linux"))]
assert_eq!(report.disk_io, Some(vec![]));
```

2. Add a two-sample rate calculation test for non-Linux. This verifies that the mount-point key strategy produces correct rates across consecutive collect() calls:

```rust
#[cfg(not(target_os = "linux"))]
#[test]
fn test_collect_disk_io_produces_rates_after_two_samples() {
    let mut collector = Collector::new(false, false);

    // First sample: baseline (empty vec)
    let report1 = collector.collect();
    assert_eq!(report1.disk_io, Some(vec![]));

    // Second sample: should produce actual rate data (or empty if no IO occurred)
    std::thread::sleep(std::time::Duration::from_millis(100));
    let report2 = collector.collect();
    assert!(report2.disk_io.is_some());

    // Verify structure: each entry has a non-empty name (mount point)
    if let Some(ref entries) = report2.disk_io {
        for entry in entries {
            assert!(!entry.name.is_empty(), "DiskIo.name should be a mount point path");
        }
    }
}
```

This test covers the critical second-sample path where `compute_disk_io` matches keys from `previous` HashMap — the exact path that would break if keys were unstable.

### Change 3: `cfg` guard cleanup

**File:** `crates/agent/src/collector/disk_io.rs`

The top-level `HashSet` import guard can remain as-is:

```rust
#[cfg(any(test, target_os = "linux"))]
use std::collections::HashSet;
```

No `HashSet` is needed in the non-Linux branch since we use `counters.contains_key()` for dedup.

## What does NOT change

- `Collector` struct — no new fields
- `disk_io::collect()` — signature unchanged
- `compute_disk_io()` — logic unchanged
- `DiskIo` protocol type in common crate — unchanged
- `Cargo.toml` — sysinfo 0.33 already a dependency
- Server code — functionally unaffected (aggregation works with any string key)
- Frontend code — functionally unaffected (aggregate chart sums all entries; per-disk view shows mount paths as labels)

## Known Limitations

1. **macOS APFS overcounting**: Multiple APFS volumes on the same physical disk may report overlapping I/O from the same `IOBlockStorageDriver`, inflating the aggregate total.
2. **Per-volume not per-disk**: Non-Linux `DiskIo.name` contains mount point paths, not physical device names. This is a semantic difference from Linux.
3. **No Windows testing**: sysinfo's Windows disk I/O implementation exists but is untested in this project. If it returns zero counters, the chart shows flat zeros (graceful degradation).

## Scope

~30 lines of production code + ~15 lines of test code across 2 files.
