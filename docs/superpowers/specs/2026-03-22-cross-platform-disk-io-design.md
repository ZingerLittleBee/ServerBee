# Cross-Platform Disk I/O Collection via sysinfo

**Date:** 2026-03-22
**Status:** Approved

## Problem

The disk I/O collector (`crates/agent/src/collector/disk_io.rs`) only works on Linux, reading `/proc/diskstats` directly. On macOS and Windows, `read_disk_counters()` returns `None`, so the DiskIO widget shows no data.

## Decision

Keep the Linux `/proc/diskstats` implementation unchanged. Add a sysinfo-based fallback for non-Linux platforms using `Disk::usage()` (available since sysinfo 0.33).

### Why not unify all platforms on sysinfo?

- ServerBee targets Linux VPS. The existing Linux implementation is battle-tested, filters physical disks cleanly (`sda`, `nvme0n1`), and excludes virtual devices.
- sysinfo's Linux IO internally reads `/proc/diskstats` anyway — wrapping it adds abstraction for zero benefit while losing whole-disk vs partition control.
- Minimal change means minimal regression risk.

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
        let name = if cfg!(target_os = "macos") {
            // macOS: use name() for APFS dedup (same strategy as disk.rs)
            disk.name().to_string_lossy().to_string()
        } else {
            // Windows/other: use mount_point to avoid merging distinct volumes with same label
            disk.mount_point().to_string_lossy().to_string()
        };

        // On macOS, skip duplicate APFS volumes (same name = same underlying disk)
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

- `total_read_bytes` / `total_written_bytes` are cumulative counters since boot, equivalent to Linux sector counts from `/proc/diskstats`.
- Uses `DiskRefreshKind::nothing().with_io_usage()` to only refresh I/O data, skipping unnecessary storage/kind queries.
- macOS uses `disk.name()` for APFS dedup (same strategy as existing `disk.rs`). Windows uses `mount_point()` to avoid merging distinct volumes with the same label.
- Returns `Some(counters)` so `collect()` enters the rate calculation branch.
- Existing `compute_disk_io()` handles rate computation — no changes needed.

### Change 2: Test assertion update

**File:** `crates/agent/src/collector/tests.rs`

Update the non-Linux assertion to match the Linux test's precision — first sample returns `Some(vec![])`:

```rust
#[cfg(not(target_os = "linux"))]
assert_eq!(report.disk_io, Some(vec![]));
```

## What does NOT change

- `Collector` struct — no new fields
- `disk_io::collect()` — signature unchanged
- `compute_disk_io()` — logic unchanged
- `DiskIo` protocol type in common crate — unchanged
- `Cargo.toml` — sysinfo 0.33 already a dependency
- Server and frontend code — unchanged

## Scope

~16 lines changed across 2 files.
