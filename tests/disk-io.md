# 磁盘 I/O 监控测试用例

## 前置条件

参照 [TESTING.md](../TESTING.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

---

## 一、自动化测试覆盖

### 单元测试

通过 `cargo test --workspace` 与 `bun run test` 运行：

| 测试组 | 文件 / 测试名 | 验证内容 |
|--------|---------------|----------|
| 协议兼容 | `crates/common/src/types.rs` / `test_system_report_without_disk_io_defaults_to_none` | 旧 payload 缺少 `disk_io` 字段时仍能反序列化，向后兼容为 `None` |
| 协议 round-trip | `crates/common/src/protocol.rs` / `test_report_with_disk_io_round_trip` | `AgentMessage::Report` 序列化/反序列化后保留 `disk_io` 数据 |
| Agent 采集语义 | `crates/agent/src/collector/tests.rs` / `test_collect_disk_io_first_sample_is_empty`、`test_collect_disk_io_first_sample_is_empty_on_non_linux` | Linux 和非 Linux 首次采样均返回空数组建立基线 |
| Agent 纯函数 | `crates/agent/src/collector/disk_io.rs` / `test_compute_disk_io_sorts_devices_and_clamps_negative_deltas`、`test_should_track_device_filters_virtual_and_partition_names`、`test_compute_disk_io_with_mount_path_keys` | 速率计算、设备名排序、计数器回退钳制、虚拟/分区设备过滤、mount-path key 速率计算 |
| Server 持久化 | `crates/server/src/service/record.rs` / `test_save_report_persists_disk_io_json`、`test_aggregate_hourly_averages_disk_io_by_device` | `disk_io_json` 原始记录持久化，小时聚合时按设备求平均 |
| 前端数据转换 | `apps/web/src/lib/disk-io.test.ts` / `parseDiskIoJson`、`buildMergedDiskIoSeries`、`buildPerDiskIoSeries` | JSON 解析容错、汇总序列构建、按磁盘补零与稳定排序 |
| 前端图表渲染 | `apps/web/src/components/server/disk-io-chart.test.tsx` / `renders merged and per-disk views`、`returns null when there is no disk I/O data` | `Merged` / `Per Disk` 视图切换、空数据时不渲染卡片 |

### 集成测试

位于 `crates/server/tests/integration.rs`：

| 测试名 | 流程 |
|--------|------|
| `test_server_records_api_returns_disk_io_json` | 注册 Agent → 直接保存带 `disk_io` 的记录 → `GET /api/servers/{id}/records` 返回 `disk_io_json`，且 JSON 可反序列化为按磁盘读写速率 |

---

## 二、E2E 手动验证

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| DI1 | 实时模式隐藏 Disk I/O | 打开 `/servers/:id` → 默认 `Real-time` 模式 | 页面中不显示 Disk I/O 卡片 | — |
| DI2 | 历史模式显示 Disk I/O | 点击 `1h`（或 `6h/24h/7d/30d`）→ 等待加载 | 显示 `Disk I/O` 卡片与 `Merged` / `Per Disk` tabs | — |
| DI3 | 汇总视图双线 | 保持 `Merged` tab → hover 图表 | Tooltip 显示 `Read` / `Write` 两条线，速率按 `KB/s` / `MB/s` / `GB/s` 格式化 | — |
| DI4 | 按磁盘视图 | 点击 `Per Disk` | 每块磁盘单独渲染图表，标题按设备名排序 | — |
| DI5 | 缺失时间点补零 | 构造部分磁盘数据 → `Per Disk` | 缺失磁盘的该时间点显示为 0，不报错不丢线 | — |
| DI6 | 时间范围切换 | 依次点击 `1h/6h/24h/7d/30d` | Disk I/O 图表时间轴和数据范围同步更新 | — |
| DI7 | 零吞吐历史可见 | read/write 全为 0 的历史记录 → 历史模式 | Disk I/O 卡片仍可见，图表显示空闲基线 | — |
| DI8 | 旧 Agent / 非 Linux 兼容 | 接入旧 agent（无 disk_io 字段）→ 历史模式 | 页面不报错，Disk I/O 区域按无数据处理；macOS/Windows agent 正常显示 | — |
| DI9 | API 返回原始 JSON | `GET /api/servers/{id}/records?interval=raw` | 响应包含 `disk_io_json`，可反序列化为每磁盘读写速率 | — |
| DI10 | i18n | 切换中文/英文 | `磁盘 I/O` / `Disk I/O`、`汇总` / `Merged`、`按磁盘` / `Per Disk`、`读取` / `Read`、`写入` / `Write` 正确 | — |
