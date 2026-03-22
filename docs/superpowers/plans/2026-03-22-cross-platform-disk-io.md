# Cross-Platform Disk I/O Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable disk I/O data collection on macOS and Windows using sysinfo's `Disk::usage()` API.

**Architecture:** Replace the `None` fallback in `read_disk_counters()` with a sysinfo-based implementation using `mount_point()` as the per-mount-path key. Linux `/proc/diskstats` path is untouched.

**Tech Stack:** Rust, sysinfo 0.33.1 (`Disk::usage()`, `DiskRefreshKind`)

**Spec:** `docs/superpowers/specs/2026-03-22-cross-platform-disk-io-design.md`

**Note:** The top-level `#[cfg(any(test, target_os = "linux"))] use std::collections::HashSet` guard in `disk_io.rs` stays as-is — the non-Linux branch uses `counters.contains_key()` instead and does not need `HashSet`.

---

### Task 1: Add deterministic unit test for mount-path keys

**Files:**
- Modify: `crates/agent/src/collector/disk_io.rs:143-220` (tests module)

- [ ] **Step 1: Write the test**

Add this test to the existing `mod tests` block in `disk_io.rs`, after the `test_should_track_device_filters_virtual_and_partition_names` test (line 219):

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

    assert_eq!(
        result,
        vec![
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
        ]
    );
}
```

- [ ] **Step 2: Run test to verify it passes**

This test exercises existing `compute_disk_io` which is already implemented — it should pass immediately since the function is key-format-agnostic. (`DiskIo` already derives `PartialEq`, as used by the existing test at line 187.)

Run: `cargo test -p serverbee-agent test_compute_disk_io_with_mount_path_keys`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/agent/src/collector/disk_io.rs
git commit -m "test(agent): add compute_disk_io test with mount-path keys"
```

---

### Task 2: Implement sysinfo fallback and update integration test

**Files:**
- Modify: `crates/agent/src/collector/disk_io.rs:38-41`
- Modify: `crates/agent/src/collector/tests.rs:58-64`

- [ ] **Step 1: Replace the non-Linux branch in `read_disk_counters()`**

In `disk_io.rs`, replace lines 38-41:

```rust
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
```

with:

```rust
    #[cfg(not(target_os = "linux"))]
    {
        use sysinfo::{DiskRefreshKind, Disks};

        // Use mount_point as key: stable between samples, unique per mount path.
        // This gives per-mount-path (not per-physical-disk) semantics.
        let disks =
            Disks::new_with_refreshed_list_specifics(DiskRefreshKind::nothing().with_io_usage());
        let mut counters = HashMap::new();

        for disk in disks.list() {
            let name = disk.mount_point().to_string_lossy().to_string();

            if counters.contains_key(&name) {
                continue;
            }

            let usage = disk.usage();
            counters.insert(
                name,
                DiskCounters {
                    read_bytes: usage.total_read_bytes,
                    write_bytes: usage.total_written_bytes,
                },
            );
        }

        Some(counters)
    }
```

- [ ] **Step 2: Update the non-Linux integration test**

In `tests.rs`, replace lines 58-64:

```rust
#[cfg(not(target_os = "linux"))]
#[test]
fn test_collect_disk_io_is_none_on_unsupported_platforms() {
    let mut collector = Collector::new(true, false);
    let report = collector.collect();
    assert!(report.disk_io.is_none());
}
```

with:

```rust
#[cfg(not(target_os = "linux"))]
#[test]
fn test_collect_disk_io_first_sample_is_empty_on_non_linux() {
    let mut collector = Collector::new(true, false);
    let report = collector.collect();
    assert_eq!(report.disk_io, Some(vec![]));
}
```

- [ ] **Step 3: Run all agent tests**

Run: `cargo test -p serverbee-agent`
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/collector/disk_io.rs crates/agent/src/collector/tests.rs
git commit -m "feat(agent): add sysinfo-based disk IO for non-Linux platforms"
```

---

### Task 3: Update TESTING.md

**Files:**
- Modify: `TESTING.md`

- [ ] **Step 1: Update the test description table**

In the Disk I/O testing section (around line 611), update the agent test descriptions:

Replace:
```
| Agent 采集语义 | `crates/agent/src/collector/tests.rs` / `test_collect_disk_io_first_sample_is_empty`、`test_collect_disk_io_is_none_on_unsupported_platforms` | Linux 首次采样返回空数组建立基线；非 Linux 平台返回 `None` |
| Agent 纯函数 | `crates/agent/src/collector/disk_io.rs` / `test_compute_disk_io_sorts_devices_and_clamps_negative_deltas`、`test_should_track_device_filters_virtual_and_partition_names` | 速率计算、设备名排序、计数器回退钳制、虚拟/分区设备过滤 |
```

with:
```
| Agent 采集语义 | `crates/agent/src/collector/tests.rs` / `test_collect_disk_io_first_sample_is_empty`、`test_collect_disk_io_first_sample_is_empty_on_non_linux` | Linux 和非 Linux 首次采样均返回空数组建立基线（非 Linux 使用 sysinfo `Disk::usage()` + mount_point key） |
| Agent 纯函数 | `crates/agent/src/collector/disk_io.rs` / `test_compute_disk_io_sorts_devices_and_clamps_negative_deltas`、`test_should_track_device_filters_virtual_and_partition_names`、`test_compute_disk_io_with_mount_path_keys` | 速率计算、设备名排序、计数器回退钳制、虚拟/分区设备过滤、mount-path key 速率计算 |
```

- [ ] **Step 2: Update the E2E verification item DI8**

Around line 636, update:

Replace:
```
| DI8 | 旧 Agent / 非 Linux 兼容 | 接入旧 agent 或非 Linux agent → 切到历史模式 → 页面不报错，Disk I/O 区域按无数据处理 | — |
```

with:
```
| DI8 | 旧 Agent / 非 Linux 兼容 | 接入旧 agent（无 disk_io 字段）→ 切到历史模式 → 页面不报错，Disk I/O 区域按无数据处理；macOS/Windows agent → 正常显示 Disk I/O 数据（name 为挂载路径） | — |
```

- [ ] **Step 3: Commit**

```bash
git add TESTING.md
git commit -m "docs: update TESTING.md for cross-platform disk IO support"
```
