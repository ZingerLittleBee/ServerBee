# Agent Self-Upgrade Closure — 设计文档

**日期**: 2026-04-14
**状态**: Draft (v2 — 根据 review 修订)
**相关实现**: `crates/common/src/protocol.rs`, `crates/server/src/router/api/server.rs`, `crates/agent/src/reporter.rs`, `crates/server/src/router/ws/agent.rs`, `crates/server/src/router/ws/browser.rs`, `crates/server/src/config.rs`, `apps/web/src/components/server/`, `apps/web/src/locales/`

## 概述

ServerBee 已经有一个"单向"的 agent 自升级通道：管理员通过 `POST /api/servers/{id}/upgrade` 让服务端发 `ServerMessage::Upgrade { version, download_url, sha256 }`，agent 收到后下载、校验、替换、重启。但该流程存在三个使它无法作为交互式运维功能暴露给管理员的缺口：

1. **无结果/进度回报** — agent 不向服务端发送升级中间状态或失败原因；管理员调用 API 后只能"盲等"。
2. **无前端 UI** — 只能 `curl` 调用，无法在 Web 控制台触发或观察。
3. **无失败回滚底线** — 老二进制被 rename 成 `.bak` 后，如果新进程 spawn 失败，agent 会彻底下线，必须人工 SSH 恢复。

本设计在现有协议基础上补齐以上三块，使自升级成为一个全链路可观测、有安全底线的运维能力。

**范围外**（显式声明）：
- 批量升级（可由前端循环单机 API 实现）
- 服务端主动确认之外的机制（现有"重连 + SystemInfo 版本匹配"已足够）
- Windows 自升级的运行中二进制覆盖问题（遗留风险，不在本次 scope）
- 升级事件的持久化审计日志（后续如有需求再加 `event` 表）

## 需求

1. Agent 在升级过程每进入一个新阶段时向服务端回报进度。
2. Agent 在失败时向服务端回报失败阶段和错误信息。
3. 服务端追踪每台 agent 当前的升级 job 状态（内存态）并在状态变化时广播给前端。
4. 服务端实现升级超时判定（agent 未能在窗口内重连并上报新版本即视为失败）。
5. Agent 在写入新二进制之前执行 Pre-flight 预检（`--version` 探活），失败则中止升级。
6. Agent 备份文件采用带时间戳命名，保留 24h。
7. 前端在 `CapabilitiesDialog` 里新增 Agent Version 分组，显示当前版本、最新版本、触发按钮，以及升级进行中/失败/成功/超时的实时状态。
8. 服务端新增 `GET /api/agent/latest-version` 端点，默认自动识别 GitHub Releases 上游，支持自托管 `latest_version_url` 覆盖，10 分钟成功缓存 / 1 分钟失败缓存。
9. 前端在 server 列表行上展示升级进行中/失败的 badge，含 WS 重连/刷新场景的首屏注入。
10. 失败态提供 Retry 按钮（admin-only），重新触发同一版本升级。
11. 服务端在 SystemInfo 更新后广播扩展的 `AgentInfoUpdated` 消息（含 `agent_version`），前端据此刷新 react-query cache，确保升级成功后"Current 版本"实时更新。
12. `BrowserMessage::FullSync` 额外携带 `upgrades: Vec<UpgradeJobDto>`，用于 WS 首连/重连时 hydrate 前端 upgrade store。
13. 服务端在每次 `start_job` 时生成 `job_id: Uuid v4`，通过 `ServerMessage::Upgrade` 下发，agent 在后续 `UpgradeProgress` / `UpgradeResult` 中回传，tracker 以 `job_id` 为主键匹配，解决"同版本重试被延迟消息串台"问题。
14. Agent 返回 `CapabilityDenied(upgrade)` 时，服务端立即将对应 Running job 翻转为 Failed（不必等 120s 超时）。
15. `AgentVersionSection` 位于 Server 详情页（而非 admin-only 的 CapabilitiesDialog），Member 只读可见，admin 额外看到触发按钮。

## 非需求

- 不做 Spawn-then-verify 握手（L2 回滚）— 协议需要新老版本双向支持，本次升级无法受益，ROI 不足。
- 不加持久化 job 表 — 升级是低频运维动作，内存态 + 超时清理足够。
- 不做升级进度百分比 — 二进制体积小，秒级完成，细粒度上报开销占比过高。
- Agent 升级成功后不主动回报 `UpgradeResult { ok: true }` — 老进程已 `exit(0)`，新进程无法关联 job；成功态以"重连 + SystemInfo 版本匹配" 推断。

## 协议变更（`crates/common/src/protocol.rs`）

### 新增 `UpgradeStage` 枚举

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeStage {
    Downloading,   // 下载中
    Verifying,     // 校验 SHA-256
    PreFlight,     // 执行 --version 预检
    Installing,    // 写临时文件 + chmod + rename
    Restarting,    // 老进程即将 exit(0)
}
```

### `ServerMessage::Upgrade` 新增 `job_id` 字段

```rust
Upgrade {
    version: String,
    download_url: String,
    sha256: String,
    #[serde(default)]
    job_id: Option<String>,   // 新增 — UUID v4，用于区分同一版本的多次重试
},
```

`Option<String>` + `#[serde(default)]` 保证跨版本兼容：
- **新 server → 旧 agent**: 旧 agent 反序列化时忽略未知字段（serde 默认行为）。
- **旧 server → 新 agent**: 缺失 `job_id` 反序列化为 `None`，新 agent 继续工作，但其发回的 progress 消息也不带 job_id；服务端 tracker 在 job_id 为 None 时降级到 `(server_id, target_version)` 匹配。
- **已知限制**: 旧 agent 场景下，同版本连续重试可能被"上一轮延迟消息"串台（target_version 相同）；这是可接受的降级，实际影响仅在混版环境。

### `AgentMessage` 新增两个变体

```rust
// 进度里程碑：进入每个阶段之前发一条
UpgradeProgress {
    msg_id: String,
    #[serde(default)]
    job_id: Option<String>,   // 透传服务端 Upgrade 消息中的 job_id
    target_version: String,
    stage: UpgradeStage,
},

// 终态（仅失败时发送）
UpgradeResult {
    msg_id: String,
    #[serde(default)]
    job_id: Option<String>,
    target_version: String,
    stage: UpgradeStage,   // 失败发生在哪一阶段
    error: String,         // 人读错误信息，UI 直接展示
},
```

**设计要点**：
- `job_id` 是 tracker 匹配的首选键；`target_version` 降级为 UI metadata + 兼容回退键。
- `UpgradeResult` 不带 `ok` 字段 — agent 只在失败时发送，"存在 = 失败"。
- 成功态不由 agent 消息表达，仅靠"重连 + `SystemInfo.agent_version == target_version`" 推断。
- `msg_id` 沿用现有 `SystemInfo` 等消息的风格（UUID v4）。

### 新增 `UpgradeStatus` 枚举

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeStatus {
    Running,
    Succeeded,
    Failed,
    Timeout,
}
```

### `BrowserMessage` 新增两个变体

服务端 → 浏览器的广播通道：

```rust
UpgradeProgress {
    server_id: String,
    job_id: String,                   // tracker 侧一定有 job_id（server 自己生成）
    target_version: String,
    stage: UpgradeStage,
},
UpgradeResult {
    server_id: String,
    job_id: String,
    target_version: String,
    status: UpgradeStatus,            // Succeeded / Failed / Timeout
    stage: Option<UpgradeStage>,      // 仅 Failed 时
    error: Option<String>,            // 仅 Failed 时
},
```

### 扩展现有 `BrowserMessage::AgentInfoUpdated`

**现状**（`crates/common/src/protocol.rs:319`）仅推 `protocol_version`，前端 `use-servers-ws.ts:239` 据此更新 react-query 缓存。

**改动**: 新增可选 `agent_version` 字段：

```rust
AgentInfoUpdated {
    server_id: String,
    protocol_version: i32,
    #[serde(default)]
    agent_version: Option<String>,    // 新增
},
```

服务端在 SystemInfo handler 更新 DB 后，广播时同时携带两个字段。前端 hook 解析 `agent_version` 并 patch `['servers', id]` / `['servers-list']` cache 中的 `agent_version` 字段。这样 `AgentVersionSection` 组件只需从 react-query cache 读当前版本，升级成功后"Current"自动刷新，无需单独订阅。

### 扩展 `BrowserMessage::FullSync`

**现状**（`crates/server/src/router/ws/browser.rs:251` `build_full_sync`）仅推 `servers: Vec<ServerStatus>`。

**改动**:

```rust
FullSync {
    servers: Vec<ServerStatus>,
    upgrades: Vec<UpgradeJobDto>,     // 新增 — 所有 Running + 24h 内终态 job
},
```

`build_full_sync` 从 `upgrade_tracker.snapshot()` 拉取。前端在收到 FullSync 时初始化/重置 upgrade store。WS 重连时 FullSync 会重发，自动 re-hydrate 列表行 badge 状态。

## 服务端改动（`crates/server`）

### 内存 Job 追踪器

**新文件**: `crates/server/src/service/upgrade_tracker.rs`

```rust
pub struct UpgradeJob {
    pub job_id: String,                        // UUID v4，server 生成
    pub server_id: String,
    pub target_version: String,
    pub started_at: DateTime<Utc>,
    pub stage: UpgradeStage,
    pub status: UpgradeStatus,
    pub error: Option<String>,
    pub finished_at: Option<DateTime<Utc>>,
}

pub struct UpgradeJobTracker {
    jobs: DashMap<String, UpgradeJob>,  // key: server_id（同 server 只允许一个活跃 job）
    browser_tx: broadcast::Sender<BrowserMessage>,
}
```

**匹配策略（重要）**：所有 agent→server 的消息查找 job 时，优先用 `job_id` 严格匹配；若 agent 消息的 `job_id` 为 None（旧版本兼容路径），降级到 `(server_id, target_version)` 匹配 + `status == Running` 门槛。

**核心方法**（所有状态变化方法内部广播 `BrowserMessage`）：

| 方法 | 行为 |
|------|------|
| `start_job(server_id, target_version) -> Result<UpgradeJob>` | DashMap `entry` 原子插入。生成 `job_id = Uuid::new_v4()`。若已存在 `Running` 状态的 job，返回 `AppError::Conflict` 带现有 job。若存在但为终态（Succeeded/Failed/Timeout），覆盖。返回新创建的 job 以便 router 传给 WS send。 |
| `update_stage(server_id, job_id_or_version, stage)` | 按匹配策略查 job；只在状态 `Running` 时更新。不匹配或已终态时 warn 日志并忽略。 |
| `mark_failed(server_id, job_id_or_version, stage, error)` | 翻转为 `Failed`，设置 `finished_at`，广播 `UpgradeResult`。 |
| `mark_failed_by_capability_denied(server_id)` | 专用入口：找当前 Running job 并 `mark_failed(Downloading, "capability denied by agent")`。无 Running 时 no-op。 |
| `mark_succeeded(server_id, observed_version)` | 仅当 `observed_version == target_version` 且状态 `Running` 时翻转为 `Succeeded`。若状态已是 `Timeout` 则不覆盖（Timeout 即终态）。 |
| `sweep_timeouts(now)` | 扫描所有 `Running` 且 `started_at + 120s < now` 的 job，翻转为 `Timeout`。 |
| `cleanup_old(now)` | 删除所有 `finished_at + 24h < now` 的终态 job，防止内存无限增长。 |
| `get(server_id) -> Option<UpgradeJob>` | 供 `GET /api/servers/{id}/upgrade` 查询当前/最近 job。 |
| `snapshot() -> Vec<UpgradeJob>` | 供 `build_full_sync` 一次性拉取所有 Running + 终态 job。 |

**装配**: 在 `AppState` 新增 `upgrade_tracker: Arc<UpgradeJobTracker>`。

### REST 路由改动

`crates/server/src/router/api/server.rs` 现有 `trigger_upgrade`（519-626）调整流程 —— **关键**: `start_job` 必须放在所有 fallible 预检之后、WS send 之前，以免创建"假 Running job"。

1. 校验 `CAP_UPGRADE`（保留）。
2. 校验 agent 在线；若离线直接 409，不创建 job。
3. 获取 agent 平台、构造资产名；若平台不支持返回 400，不创建 job。
4. 拉 `checksums.txt` 并解析 SHA-256；网络/解析失败返回 502，不创建 job。
5. **此时才调用** `tracker.start_job(server_id, target_version)` —— 若 `Conflict`（已有 Running job）返回 409 + 现有 job DTO。成功后拿到带 `job_id` 的 `UpgradeJob`。
6. 通过 AgentManager 发送 `ServerMessage::Upgrade { ..., job_id: Some(job.job_id.clone()) }`。
7. 若 WS 发送失败，立即调 `tracker.mark_failed(job.job_id, Downloading, "failed to notify agent: ...")` 并返回 500。虽然会留一个短暂的 Running → Failed 切换，这是预期的：UI 会看到一次失败广播，符合"所有失败都有可见反馈"的原则。
8. 成功返回 202 Accepted + `{ data: { job: UpgradeJobDto } }`，前端直接拿到初始 `Running` 状态。

**并发保护**：两个 POST 同时到达时，步骤 2-4 都能各自完成（浪费一次 checksum 拉取），第 5 步 DashMap `entry` 原子性保证只有一个能 `start_job`，另一个拿 409。

**新增路由**: `GET /api/servers/{id}/upgrade` → 返回 `Option<UpgradeJobDto>`（当前或最近 24h 内的终态 job）。挂载在 `read_router()` 下（Admin + Member 都可读）。

### Agent WS 消息处理

`crates/server/src/router/ws/agent.rs` 现有 match 分支新增 / 修改：

- **新增** `AgentMessage::UpgradeProgress { job_id, target_version, stage, .. }` → `tracker.update_stage(server_id, job_id_or_version(...), stage)`
- **新增** `AgentMessage::UpgradeResult { job_id, target_version, stage, error, .. }` → `tracker.mark_failed(server_id, job_id_or_version(...), stage, error)`
- **修改** `AgentMessage::SystemInfo { info, .. }` 现有处理末尾加钩子：
  1. 若 `tracker.get(server_id)` 为 `Running` 且 `info.agent_version == target_version` → `tracker.mark_succeeded(server_id, info.agent_version)`
  2. **新增**: DB 更新完成后，广播 `BrowserMessage::AgentInfoUpdated { server_id, protocol_version, agent_version: Some(info.agent_version.clone()) }`
- **修改** `AgentMessage::CapabilityDenied` 当前分支（`agent.rs:586-594`）处理 "upgrade" capability：若 `capability == "upgrade"`，调用 `tracker.mark_failed_by_capability_denied(server_id)`，让 UI 立即看到失败（而非等 120s 超时）。其他 capability 的现有处理逻辑保持不变。

### 超时 / 清理后台任务

**新文件**: `crates/server/src/task/upgrade_timeout.rs`

每 10s tick 一次，执行 `tracker.sweep_timeouts(now)` 和 `tracker.cleanup_old(now)`。挂到 `main.rs` 的后台任务启动序列（和 `record_writer` / `alert_evaluator` 同级）。

### 新增 REST: `GET /api/agent/latest-version`

**目的**: 前端需要"最新版本号"才能决定是否显示升级按钮。

**上游协议**（默认 `release_base_url = https://github.com/ZingerLittleBee/ServerBee/releases`，见 `crates/server/src/config.rs:274-282`）：

策略 (C) — 自动识别 GitHub + 可选 override：
1. **Override**: 若配置 `upgrade.latest_version_url` 显式设置（新增 optional 字段），直接调用该 URL，期望返回 JSON `{ version: "x.y.z", released_at?: "..." }`。
2. **Auto-detect**: 否则解析 `release_base_url`，若匹配正则 `^https://github\.com/([^/]+)/([^/]+)/releases/?$`，自动调用 GitHub API：`GET https://api.github.com/repos/{owner}/{repo}/releases/latest`，提取 `tag_name`（剥掉可选 `v` 前缀）作为版本号，`published_at` 作为 released_at。
3. **Neither matches**: 返回 `{ version: null, error: "auto-detect failed; set upgrade.latest_version_url" }`，前端据此隐藏升级按钮并显示引导文案。

**实现细节**:
- 内存缓存（OnceCell + Mutex 或 `moka`）成功响应 TTL 10 分钟。
- 失败响应 TTL 1 分钟，避免打爆上游。
- GitHub API 未认证配额 60 req/hour per IP — 10 分钟缓存下单实例每小时最多 6 次调用，远低于限制。
- HTTP client 使用现有 `reqwest` 实例；超时 10s；User-Agent 带 `serverbee-server/<version>`。
- 挂载在 `read_router()`（Admin + Member 都可读）。

**新增 config** (`crates/server/src/config.rs`):

```rust
pub struct UpgradeConfig {
    #[serde(default = "default_release_base_url")]
    pub release_base_url: String,
    #[serde(default)]
    pub latest_version_url: Option<String>,    // 新增 — 自托管覆盖
}
```

对应环境变量：`SERVERBEE_UPGRADE__LATEST_VERSION_URL`。需同步更新 `ENV.md` 和 `apps/docs/content/docs/{en,cn}/configuration.mdx`。

## Agent 改动（`crates/agent`）

### 进度上报工具

`Reporter` 新增两个私有方法：

```rust
async fn emit_upgrade_progress(&self, target_version: &str, stage: UpgradeStage);
async fn emit_upgrade_failure(&self, target_version: &str, stage: UpgradeStage, error: String);
```

`emit_upgrade_progress` 发送失败仅记 `warn` 日志（进度丢失不致命）。`emit_upgrade_failure` 尽最大努力 flush（失败消息尽量送达，但也不阻塞升级本体）。

### `perform_upgrade` 重构后的阶段序列

`crates/agent/src/reporter.rs:1776-1846` 重构为：

```
emit(Downloading)
├─ reqwest 下载到 <binary>.new（10 分钟超时）
│  └─ 失败 → emit_failure(Downloading, err) → return

emit(Verifying)
├─ SHA-256 校验
│  └─ 不匹配 → emit_failure(Verifying, "sha256 mismatch") → 删 .new → return

emit(PreFlight)                              ← 新增 L1
├─ chmod 0755（Unix）
├─ Command::new(".new").arg("--version").status() with 5s timeout
│  └─ 失败 → emit_failure(PreFlight, "preflight failed: ...") → 删 .new → return

emit(Installing)
├─ rename <binary> → <binary>.bak.<timestamp>
├─ rename <binary>.new → <binary>
│  └─ 失败 → emit_failure(Installing, err) → 尝试 rename .bak.<timestamp> → <binary> 回滚 → return

emit(Restarting)
├─ Command::new(<binary>).spawn()
│  └─ 失败 → emit_failure(Restarting, err) → rename .bak.<timestamp> → <binary> 恢复 → return
├─ flush WS sender（尽力把 Restarting 消息送出）
├─ exit(0)
```

**纯函数抽离**（便于单元测试）：
- `fn verify_sha256(bytes: &[u8], expected: &str) -> Result<()>`
- `async fn run_preflight(path: &Path, timeout: Duration) -> Result<()>`

### `.bak` 保留窗口

- **命名**: `<binary_path>.bak.<YYYYMMDD-HHMMSS>`，避免连续升级覆盖上一个能跑的版本。
- **清理**: agent 启动时调 `cleanup_old_backups(binary_dir)`，删除同目录下 `.bak.*` 且 `mtime` 早于 24h 的文件。
- 磁盘占用上限估算：单个二进制 5-15 MB × 24h 内连续升级不超过 10 次 ≈ 150 MB 上限，可接受。

### 并发升级保护

若 agent 正在升级过程中再次收到 `ServerMessage::Upgrade`，立即 `emit_failure(Downloading, "upgrade already in progress")` 回写失败，让服务端能看到重复请求被拒。

### Capability 检查

保留现有 `reporter.rs:576-592` 的逻辑 — 消息 dispatch 阶段统一检查，升级流程内部不再查。

### Windows 兼容性

- chmod 已有 `#[cfg(unix)]` 隔离。
- `--version` 调用和 rename 语义跨平台一致。
- **已知限制**: Windows 运行中的 exe 不能被 rename/delete，现状未验证。本设计不引入新的 Windows 问题，但也不解决该遗留风险 — 在文档里标注"Windows 自升级未验证，请谨慎"。

## 前端改动（`apps/web`）

### 全局 upgrade job store

**方式**: 新建 `apps/web/src/stores/upgrade-jobs-store.ts`，使用 **zustand**（如现有代码已有 zustand usage 则沿用；否则退化为 React Context + reducer；plan 阶段先 grep 确认）。

**State 形状**:

```ts
type UpgradeJob = {
  jobId: string
  serverId: string
  targetVersion: string
  stage: 'downloading' | 'verifying' | 'pre_flight' | 'installing' | 'restarting'
  status: 'running' | 'succeeded' | 'failed' | 'timeout'
  error?: string
  startedAt: string
  finishedAt?: string
}

type UpgradeJobsStore = {
  jobs: Record<string, UpgradeJob>    // key: serverId
  setJobs: (jobs: UpgradeJob[]) => void           // FullSync 批量替换（整体覆盖；未出现在新列表的 serverId 被删除）
  upsertJob: (job: UpgradeJob) => void            // 单条更新；相同 (serverId, jobId, status) 的重复消息需要去重，避免 WS 回放导致 UI 闪烁
  clearFinished: (serverId: string) => void       // 成功态 3 秒后调用
}
```

### 新增 hook

`apps/web/src/hooks/use-upgrade-job.ts`

- `useUpgradeJob(serverId)`: 从 store 选取单台 server 的 job。首次 mount 时如果 store 中没有对应 serverId 的条目，调 `GET /api/servers/{id}/upgrade` 兜底获取（覆盖"从外部链接直接进入 server 详情页"的场景）。
- `useTriggerUpgrade()`: mutation，POST `/api/servers/{id}/upgrade`，成功后把返回的 `job` 写入 store（`upsertJob`）。

### WS 消息路由

`apps/web/src/hooks/use-servers-ws.ts` 现有消息 switch 新增 / 修改 case：

- **新增** `upgrade_progress` / `upgrade_result` → `store.upsertJob(...)`
- **扩展** `agent_info_updated`（现有 l.239）: 除了 `protocol_version`，额外 patch `agent_version`（如果消息带此字段）到 `['servers', id]` 和 `['servers-list']` cache。
- **扩展** `full_sync`: 解析新增的 `upgrades` 数组，调 `store.setJobs(upgrades)`。

### UI 组件

**位置变更（修订）**: `AgentVersionSection` **不放**在 admin-only 的 `CapabilitiesDialog` 里，改放在 **Server 详情页**（`apps/web/src/routes/_authed/servers/$serverId.*.tsx` 的合适子区块，实现时确认具体路由文件名）作为独立的信息区。理由：

- `GET /api/servers/{id}/upgrade` 对 Member 可读，Member 需要能看到"本机正在升级 / 刚升级失败"的完整上下文（列表 badge 点进去要能看到原因）。
- `CapabilitiesDialog` hard-gate `user?.role !== 'admin'` 会把整个 section 对 Member 隐藏，与 GET 端点可读性不一致。

**渲染分层**:
- 所有角色：看到 `Current: vX.Y.Z`、`Latest: vY.Y.Z`（或 "Up to date"）、Running/Failed/Timeout 的完整 stepper/文案。
- 仅 admin：额外渲染 `[ Upgrade to vY.Y.Z ]` 按钮和失败态的 `Retry` 按钮。Member 在 idle 态看不到操作按钮；在 Failed 态看到原因但无法重试。

**新建文件**: `apps/web/src/components/server/agent-version-section.tsx`。

**三个显示态**:

**A. Idle**（无活跃 job）
- 展示 `Current: vX.Y.Z`
- 若 `latest-version` API 返回有效版本且 > Current：显示 `[ Upgrade to vLatest ]` 按钮
- 若 `latest-version` 返回 null 或解析失败：隐藏按钮 + 显示 `"Auto upgrade not configured. See docs."`
- 若 CAP_UPGRADE 未启用：按钮 disabled + tooltip `"Enable Upgrade capability first"`
- 若 `Current == Latest`：显示 `"Up to date"`
- 点击按钮 → 二次确认 dialog: *"Upgrading will disconnect the agent for up to 2 minutes. Proceed?"*

**B. Running**
- Stepper 显示 5 阶段：`Downloading / Verifying / Pre-flight / Installing / Restarting`
- 当前阶段高亮，已完成阶段 ✓
- Restarting 阶段无后续消息，UI 保持该步激活直到收到终态或超时

**C. 终态**
- **Succeeded**: 绿色 ✓ `"Upgraded to vX.Y.Z — <时间>"`，3 秒后自动回到 Idle
- **Failed**: 红色 ✗ `"Failed at <stage>: <error>"` + **Retry** 按钮 + 文案 `"Previous binary kept at <binary>.bak.<timestamp> for 24h."`
- **Timeout**: 橙色 `"Agent did not reconnect within 2 minutes. It may still be restarting; check back shortly."`

### Server 列表行 badge

`apps/web/src/components/server/server-list` 或对应列表组件订阅全局 upgrade job store：
- Running 态 → 行尾小 badge `"Upgrading..."`
- Failed 态 → 红色 badge `"Upgrade failed"`（可点击跳到 Server 详情页的 AgentVersionSection 查看原因）

### i18n

**修订**: `apps/web` 已接入 react-i18next（参考 `capabilities-dialog.tsx:55` `t('cap_group_low_risk', ...)`），文案分别在 `apps/web/src/locales/en/servers.json` 和 `apps/web/src/locales/zh/servers.json`。

**改动**: 新增文案 key 同时写入 `en/servers.json` 和 `zh/servers.json`（或新建 `upgrade` namespace），至少包括：

| Key | EN | 中文 |
|---|---|---|
| `upgrade.section_title` | Agent Version | Agent 版本 |
| `upgrade.current` | Current | 当前版本 |
| `upgrade.latest` | Latest | 最新版本 |
| `upgrade.up_to_date` | Up to date | 已是最新 |
| `upgrade.button` | Upgrade to {{version}} | 升级到 {{version}} |
| `upgrade.confirm_title` | Confirm upgrade | 确认升级 |
| `upgrade.confirm_body` | Upgrading will disconnect the agent for up to 2 minutes. Proceed? | 升级将使 agent 断连最长 2 分钟，是否继续？ |
| `upgrade.disabled_cap` | Enable Upgrade capability first | 请先开启升级能力 |
| `upgrade.not_configured` | Auto upgrade not configured. See docs. | 未配置自动升级，请参考文档 |
| `upgrade.stage.downloading` | Downloading | 下载中 |
| `upgrade.stage.verifying` | Verifying | 校验中 |
| `upgrade.stage.pre_flight` | Pre-flight | 预检中 |
| `upgrade.stage.installing` | Installing | 安装中 |
| `upgrade.stage.restarting` | Restarting | 重启中 |
| `upgrade.succeeded` | Upgraded to {{version}} | 已升级到 {{version}} |
| `upgrade.failed` | Failed at {{stage}}: {{error}} | {{stage}} 阶段失败：{{error}} |
| `upgrade.failed_hint` | Previous binary kept at {{path}} for 24h. | 旧版本保留在 {{path}}（24 小时） |
| `upgrade.timeout` | Agent did not reconnect within 2 minutes. It may still be restarting; check back shortly. | Agent 未在 2 分钟内重连，可能仍在重启，请稍后查看 |
| `upgrade.retry` | Retry | 重试 |
| `upgrade.badge_running` | Upgrading... | 升级中… |
| `upgrade.badge_failed` | Upgrade failed | 升级失败 |

## 错误处理 & 边界

### 服务端边界

| 场景 | 行为 |
|------|------|
| POST 时 agent 离线 | 409 Conflict，**不创建 job**（预检在 start_job 之前） |
| POST 时平台不支持 | 400 Bad Request，**不创建 job** |
| POST 时 checksums 拉取 / 解析失败 | 502 Bad Gateway，**不创建 job** |
| POST 时已有 Running job | 409 Conflict + 返回现有 job |
| POST 时 CAP_UPGRADE 未启用 | 403 Forbidden（现有逻辑保留） |
| WS 发送 Upgrade 消息失败 | `mark_failed(Downloading, "failed to notify agent")` + 500（已有 Running job，会产生一次 Running→Failed 广播，UI 能看到） |
| 收到 UpgradeProgress 但 job_id 不匹配 | 忽略 + warn 日志 |
| 收到 UpgradeProgress 但 job_id 缺失（旧 agent）且 target_version 不匹配 | 忽略 + warn 日志 |
| 收到 UpgradeProgress 但无活跃 job | 忽略 + warn 日志 |
| 收到 UpgradeResult 但 job 已 Succeeded/Timeout | 忽略 |
| Agent 升级中 WS 断开 | 不立即判失败（Restarting 阶段会断） |
| Agent 返回 CapabilityDenied(upgrade) | 立即 `mark_failed_by_capability_denied(server_id)`，UI 即时看到失败原因 |
| Agent 重连但版本号仍是旧的 | job 保持 Running，等超时 |
| Agent 重连但版本号非 target | `mark_failed(Restarting, "agent reconnected with unexpected version X, expected Y")` |
| SystemInfo 到达后 | DB 更新 + 广播 `AgentInfoUpdated { agent_version }`；如匹配 target_version 则触发 `mark_succeeded` |
| `latest-version` 拉取失败 | API 返回 `{ version: null, error }`，不 500；缓存空结果 1 分钟 |
| 超时后 SystemInfo 匹配 | **不翻转回 Succeeded**；Timeout 即终态（UI 里 Timeout + 新版本号可并存，由用户自行判断） |

### Agent 边界

| 场景 | 行为 |
|------|------|
| 下载 HTTP 非 200 | `emit_failure(Downloading, "http {status}")` + 清理 .new |
| 下载中断 | `emit_failure(Downloading, "io: ...")` |
| SHA-256 不匹配 | `emit_failure(Verifying, "sha256 mismatch: got X, want Y")` + 删 .new |
| PreFlight 退出码非 0 / 超时 | `emit_failure(PreFlight, "...")` + 删 .new |
| Installing rename 失败 | `emit_failure(Installing, ...)` + 尝试从 .bak.<ts> 回滚 |
| Spawn 新进程失败 | `emit_failure(Restarting, ...)` + 从 .bak.<ts> 恢复 + 老进程继续跑 |
| emit_failure WS 发送失败 | 记本地日志；服务端靠超时兜底 |
| 收到 Upgrade 但 CAP_UPGRADE 关闭 | 发 `CapabilityDenied`（现有逻辑保留） |
| 升级中收到第二条 Upgrade | `emit_failure(Downloading, "upgrade already in progress")` |
| 管理员删了 .bak.<ts> | 不影响任何流程（备份是 best-effort） |

### 并发 / 竞态

- **多 admin 同时 POST**: DashMap `entry` 原子插入，第二个请求 409。
- **升级中服务端重启**: 内存 job 全丢。Agent 侧消息发到已重启的 server → `update_stage` 找不到 job 会 warn 并忽略。Agent 最终重连上报 SystemInfo，`server.agent_version` 被正常更新，但 server 不知道这是升级成果。这是可接受的降级（数据最终一致）。

## 日志 / 可观测性

- 服务端：所有 job 状态变化 `tracing::info!` 含 server_id / target_version / stage。
- Agent：每个 emit 前 `tracing::info!`；失败用 `error!`。
- 不加 metrics — 升级是低频事件，现有 tracing 足够。

## 测试策略

### 单元测试

**`crates/common/src/protocol.rs`** (~8 tests)
- `UpgradeStage` / `UpgradeStatus` / `UpgradeProgress` / `UpgradeResult`（AgentMessage + BrowserMessage）serde roundtrip。
- `ServerMessage::Upgrade` 带 `job_id: Some` 和省略 `job_id` 字段两种输入都能正确反序列化（向后兼容断言）。
- `AgentMessage::UpgradeProgress` 缺失 `job_id` 时 `#[serde(default)]` 生效为 `None`。
- `BrowserMessage::AgentInfoUpdated` 带/不带 `agent_version` 都能反序列化。

**`crates/server/src/service/upgrade_tracker.rs`**（新建，~12 tests）
- `start_job` 成功路径 / 并发 Conflict / 旧终态 job 可被覆盖 / 返回的 job 带 UUID v4 `job_id`
- `update_stage` 以 `job_id` 匹配 Running job → 更新；`job_id` 不匹配 → 忽略
- `update_stage` job_id 缺失（None）时退化到 target_version 匹配
- `mark_failed_by_capability_denied` 仅对 Running job 生效，非 Running 时 no-op
- `mark_succeeded` 在 Timeout 态下不覆盖
- `sweep_timeouts` 仅翻转 Running，严格按 120s 阈值
- `cleanup_old` 按 24h 阈值删除终态 job
- `snapshot()` 返回所有 Running + 未过期终态 job
- 广播断言：每次状态变化 subscriber 都收到对应 `BrowserMessage`

**`crates/agent/src/reporter.rs`**
- 抽离的 `verify_sha256` 和 `run_preflight` 纯函数单测。
- `perform_upgrade` 本体的 emit 序列验证放到集成测试。

### 集成测试

**`crates/server/tests/integration/upgrade.rs`**（新建，~10 tests）

使用现有 `crates/server/tests/integration/` 测试夹具（具体形态在实现时确认，可能需要模拟 agent WS 连接）：

1. **成功路径**: 模拟 agent 发 Downloading → Verifying → PreFlight → Installing → Restarting → 断开 → 重连 + SystemInfo(new_version) → 断言 `status=Succeeded`，并断言 `AgentInfoUpdated` 广播中包含 `agent_version=new_version`
2. **失败路径**: 模拟 agent 发 Downloading → UpgradeResult(Verifying, "sha256 mismatch") → 断言 `status=Failed, stage=Verifying`
3. **超时路径**: 模拟 agent 发 Downloading → Restarting → 断开不再重连 → 快进 121s → 断言 `status=Timeout`
4. **并发拒绝**: 连续两次 POST /upgrade → 第二次 409
5. **pre-check 失败不创建 job**: agent 离线时 POST → 409，tracker 中无 job
6. **checksum 404 不创建 job**: 配置错误的 release_base_url → 502，tracker 中无 job
7. **job_id 错位**: POST 创建 job A，之后 agent 发 UpgradeProgress 带旧 job_id → 忽略，job A 状态不变
8. **同版本重试保护**: POST 触发 v1.0（job_A），agent 失败；POST 再次触发 v1.0（job_B，job_id 不同）；此时模拟 job_A 的延迟 UpgradeResult 到达 → 被忽略，job_B 不受影响
9. **CapabilityDenied 快速失败**: POST 触发升级后 agent 立即发 `CapabilityDenied(capability="upgrade")` → job 立即 Failed，不等超时
10. **FullSync 携带 upgrades**: 有 1 个 Running job + 1 个 Failed job 时，新浏览器订阅 WS 得到的 FullSync 包含这两个 job

### 前端测试（vitest，~10 tests）

- `upgrade-jobs-store.test.ts`: `setJobs` 批量初始化 / `upsertJob` 单条更新 / `clearFinished` 只清理终态
- `use-upgrade-job.ts`: mock WS 消息 + store 交互，断言 state 转换（idle → running → succeeded / failed / timeout）
- `AgentVersionSection.test.tsx`:
  - Admin + idle 状态：显示 "Upgrade to vX" 按钮
  - Member + idle 状态：**不显示**按钮，Current/Latest 仍可见
  - CAP_UPGRADE 关闭时 admin 按钮 disabled + tooltip
  - Running 态显示 stepper，当前阶段高亮
  - Failed 态 admin 显示 Retry 按钮；Member 只看到失败原因
  - latest-version API 返回 null 时隐藏按钮 + 显示引导文案
- `use-servers-ws.ts`: `agent_info_updated` 携带 `agent_version` 时正确 patch react-query cache
- `use-servers-ws.ts`: `full_sync` 的 `upgrades` 数组调 `store.setJobs`

### E2E 手动清单

新增 `tests/agent-upgrade.md`：
- 真实升级一台 agent 到新版本
- 故意提供错 SHA 触发 Verifying 失败
- 关 agent 网络制造超时
- 点 Retry 重试
- 验证 `.bak.<ts>` 文件存在、24h 后清理

### 显式不测

- reqwest / 网络层
- sha2 crate 自身
- Windows 升级路径（遗留风险）
- `.bak` 磁盘占用上限（用户级运维）

## 配置 / 环境变量

**新增 1 个可选环境变量**:

- `SERVERBEE_UPGRADE__LATEST_VERSION_URL` (optional) — 自托管用户可显式指定最新版本查询 URL。期望返回 JSON `{ version: "x.y.z", released_at?: "..." }`。未设置时自动识别 `upgrade.release_base_url`（现有 `SERVERBEE_UPGRADE__RELEASE_BASE_URL`）是否为 GitHub Releases 格式并调用 GitHub API。

**需同步更新**:
- `ENV.md`
- `apps/docs/content/docs/en/configuration.mdx`
- `apps/docs/content/docs/cn/configuration.mdx`

## 迁移

**无需数据库 migration**。所有新增状态在内存中追踪。

## 风险 & 未来工作

### 已知限制

1. **Windows 未验证**: 运行中的 exe 无法被 rename/delete；本设计不解决该遗留问题。
2. **服务端重启丢失活跃 job**: 内存态不持久化。可接受的降级：最终 `server.agent_version` 会被 SystemInfo 正常更新，但 UI 看不到升级确认。
3. **Timeout 后的"迟到成功"不翻转**: 如果 agent 在 120s 后才重连并上报新版本，UI 会同时看到"Timeout 状态"和"新版本号"。这是故意为之，避免引入"late success"概念。
4. **旧版本 agent 的同版本重试保护降级**: 若 agent 未升级到支持 `job_id` 的版本，服务端与 agent 之间的 job 匹配退化到 `(server_id, target_version)`。在此模式下连续重试同一版本时，第一次失败的"延迟消息"理论上可能污染第二次尝试。实际影响范围：仅混版（server 已升级但 agent 未升级）；升级一次 agent 到新协议后此限制自动解除。
5. **GitHub API 速率限制**: 未认证的 GitHub API 配额 60 req/hour per IP。10 分钟缓存下单实例远低于限制；高密度部署（如单 IP 大量容器）可能触及，需要用户自行设置 `latest_version_url` 指向内部镜像。

### 可能的后续工作

- **批量升级**: 前端批量 POST 循环 + 进度聚合视图。
- **升级事件审计**: 新增 `event` 或 `agent_upgrade_log` 表，记录每次升级的触发者、时间、结果，供事后审计。
- **L2 Spawn-then-verify 回滚**: 新老二进制双向协议支持，彻底解决"能启动但跑不起来"的 corner case。本次不做因 ROI 不足。
- **Windows 升级验证 + 中继脚本方案**: 用 `MoveFileEx(MOVEFILE_DELAY_UNTIL_REBOOT)` 或外部 updater.exe 解决 Windows 自升级问题。
