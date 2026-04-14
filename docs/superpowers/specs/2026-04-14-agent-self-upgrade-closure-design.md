# Agent Self-Upgrade Closure — 设计文档

**日期**: 2026-04-14
**状态**: Draft
**相关实现**: `crates/common/src/protocol.rs`, `crates/server/src/router/api/server.rs`, `crates/agent/src/reporter.rs`, `apps/web/src/components/server/`

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
8. 服务端新增 `GET /api/agent/latest-version` 端点，代理查询 `release_base_url` 的最新版本信息（10 分钟缓存）。
9. 前端在 server 列表行上展示升级进行中/失败的 badge。
10. 失败态提供 Retry 按钮，重新触发同一版本升级。

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

### `AgentMessage` 新增两个变体

```rust
// 进度里程碑：进入每个阶段之前发一条
UpgradeProgress {
    msg_id: String,
    target_version: String,
    stage: UpgradeStage,
},

// 终态（仅失败时发送）
UpgradeResult {
    msg_id: String,
    target_version: String,
    stage: UpgradeStage,   // 失败发生在哪一阶段
    error: String,         // 人读错误信息，UI 直接展示
},
```

**设计要点**：
- `target_version` 在每条消息中冗余携带，服务端用来防止"上一轮 Timeout 后开启的新 job" 被旧消息污染。
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
    target_version: String,
    stage: UpgradeStage,
},
UpgradeResult {
    server_id: String,
    target_version: String,
    status: UpgradeStatus,            // Succeeded / Failed / Timeout
    stage: Option<UpgradeStage>,      // 仅 Failed 时
    error: Option<String>,            // 仅 Failed 时
},
```

## 服务端改动（`crates/server`）

### 内存 Job 追踪器

**新文件**: `crates/server/src/service/upgrade_tracker.rs`

```rust
pub struct UpgradeJob {
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

**核心方法**（所有状态变化方法内部广播 `BrowserMessage`）：

| 方法 | 行为 |
|------|------|
| `start_job(server_id, target_version)` | DashMap `entry` 原子插入。若已存在 `Running` 状态的 job，返回 `AppError::Conflict`。若存在但为终态（Succeeded/Failed/Timeout），覆盖。 |
| `update_stage(server_id, target_version, stage)` | 只在 `target_version` 匹配且状态 `Running` 时更新。不匹配时 warn 日志并忽略。 |
| `mark_failed(server_id, target_version, stage, error)` | 翻转为 `Failed`，设置 `finished_at`，广播 `UpgradeResult`。 |
| `mark_succeeded(server_id, observed_version)` | 仅当 `observed_version == target_version` 且状态 `Running` 时翻转为 `Succeeded`。若状态已是 `Timeout` 则不覆盖（Timeout 即终态）。 |
| `sweep_timeouts(now)` | 扫描所有 `Running` 且 `started_at + 120s < now` 的 job，翻转为 `Timeout`。 |
| `cleanup_old(now)` | 删除所有 `finished_at + 24h < now` 的终态 job，防止内存无限增长。 |
| `get(server_id) -> Option<UpgradeJob>` | 供 `GET /api/servers/{id}/upgrade` 查询当前/最近 job。 |

**装配**: 在 `AppState` 新增 `upgrade_tracker: Arc<UpgradeJobTracker>`。

### REST 路由改动

`crates/server/src/router/api/server.rs` 现有 `trigger_upgrade`（519-626）调整流程：

1. 校验 `CAP_UPGRADE`（保留）。
2. **新增**: `tracker.start_job(server_id, target_version)`；若 `Conflict` 返回 409 + 返回现有 job DTO。
3. 获取 agent 平台、拉 `checksums.txt`、解析 SHA-256（保留）。
4. 通过 AgentManager 发送 `ServerMessage::Upgrade`（保留）。
5. **新增**: 若发送失败，`tracker.mark_failed(Downloading, "failed to notify agent: ...")`，返回 500。
6. **新增**: 成功返回 202 Accepted + `{ data: { job: UpgradeJobDto } }`，前端直接拿到初始 `Running` 状态。

**新增路由**: `GET /api/servers/{id}/upgrade` → 返回 `Option<UpgradeJobDto>`（当前或最近 24h 内的终态 job）。挂载在 `read_router()` 下（Admin + Member 都可读）。

### Agent WS 消息处理

`crates/server/src/router/ws/agent.rs`（具体文件需在实现时确认）现有 match 分支新增：

- `AgentMessage::UpgradeProgress { target_version, stage, .. }` → `tracker.update_stage(server_id, target_version, stage)`
- `AgentMessage::UpgradeResult { target_version, stage, error, .. }` → `tracker.mark_failed(server_id, target_version, stage, error)`
- `AgentMessage::SystemInfo { info, .. }` 现有处理末尾加钩子：若 `tracker.get(server_id)` 为 `Running` 且 `info.agent_version == target_version` 则 `mark_succeeded`。

### 超时 / 清理后台任务

**新文件**: `crates/server/src/task/upgrade_timeout.rs`

每 10s tick 一次，执行 `tracker.sweep_timeouts(now)` 和 `tracker.cleanup_old(now)`。挂到 `main.rs` 的后台任务启动序列（和 `record_writer` / `alert_evaluator` 同级）。

### 新增 REST: `GET /api/agent/latest-version`

**目的**: 前端需要"最新版本号"才能决定是否显示升级按钮。

**实现**:
- 新 handler，内存缓存（OnceCell + Mutex 或 `moka`）TTL 10 分钟。
- 服务端从 `config.release_base_url` 拼 `{base}/latest.txt`（或 `latest.json`，具体格式待实现时决定），拉取并解析版本号。
- 成功返回 `{ version: "0.9.0", released_at: ISO8601? }`。
- 失败（网络错误 / 未配置 `release_base_url` / 解析失败）返回 `{ version: null, error: "..." }`，**不抛 500**。
- 失败响应缓存 1 分钟，避免打爆上游或重复失败。
- 挂载在 `read_router()`。

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

### 新增 hook

`apps/web/src/hooks/use-upgrade-job.ts`

```ts
type UpgradeJob = {
  serverId: string
  targetVersion: string
  stage: 'downloading' | 'verifying' | 'pre_flight' | 'installing' | 'restarting'
  status: 'running' | 'succeeded' | 'failed' | 'timeout'
  error?: string
  startedAt: string
  finishedAt?: string
}
```

两个 hook：
- `useUpgradeJob(serverId)`:
  - 首次 mount 调 `GET /api/servers/{id}/upgrade` 同步初始状态。
  - 订阅现有全局 WS（`use-servers-ws.ts`）新增的 `upgrade_progress` / `upgrade_result` 消息，匹配 serverId 后更新本地 state。
- `useTriggerUpgrade()`:
  - mutation，POST `/api/servers/{id}/upgrade`。
  - 成功后把返回的 `job` 写入全局 store（便于列表 badge 立即展示）。

### WS 消息路由

`apps/web/src/hooks/use-servers-ws.ts` 现有消息 switch 新增两个 case，dispatch 到全局 upgrade job store。具体 store 方式（zustand / tanstack-query / context）按现有代码风格就近匹配，属实现细节。

### UI 组件

**位置**: `CapabilitiesDialog` 底部独立一个 `AgentVersionSection` 子组件（新建 `apps/web/src/components/server/agent-version-section.tsx`）。

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
- Failed 态 → 红色 badge `"Upgrade failed"`（可点击跳到 CapabilitiesDialog 查看详情）

### i18n

本次按英文单语处理（`apps/web` 暂未接入 i18n 框架）。如后续接入，新增文案 key 统一提取。

## 错误处理 & 边界

### 服务端边界

| 场景 | 行为 |
|------|------|
| POST 时 agent 离线 | 409 Conflict，不创建 job |
| POST 时已有 Running job | 409 Conflict + 返回现有 job |
| POST 时 CAP_UPGRADE 未启用 | 403 Forbidden（现有逻辑保留） |
| WS 发送 Upgrade 消息失败 | `mark_failed(Downloading, "failed to notify agent")` + 500 |
| 收到 UpgradeProgress 但 target_version 不匹配 | 忽略 + warn 日志 |
| 收到 UpgradeProgress 但无活跃 job | 忽略 + warn 日志 |
| 收到 UpgradeResult 但 job 已 Succeeded/Timeout | 忽略 |
| Agent 升级中 WS 断开 | 不立即判失败（Restarting 阶段会断） |
| Agent 重连但版本号仍是旧的 | job 保持 Running，等超时 |
| Agent 重连但版本号非 target | `mark_failed(Restarting, "agent reconnected with unexpected version X, expected Y")` |
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

**`crates/common/src/protocol.rs`** (~6 tests)
- `UpgradeStage` / `UpgradeStatus` / `UpgradeProgress` / `UpgradeResult`（AgentMessage + BrowserMessage）serde roundtrip。

**`crates/server/src/service/upgrade_tracker.rs`**（新建，~10 tests）
- `start_job` 成功路径 / 并发 Conflict / 旧终态 job 可被覆盖
- `update_stage` target_version 不匹配时忽略
- `mark_succeeded` 在 Timeout 态下不覆盖
- `sweep_timeouts` 仅翻转 Running，严格按 120s 阈值
- `cleanup_old` 按 24h 阈值删除终态 job
- 广播断言：每次状态变化 subscriber 都收到对应 `BrowserMessage`

**`crates/agent/src/reporter.rs`**
- 抽离的 `verify_sha256` 和 `run_preflight` 纯函数单测。
- `perform_upgrade` 本体的 emit 序列验证放到集成测试。

### 集成测试

**`crates/server/tests/integration/upgrade.rs`**（新建，~7 tests）

使用现有 `crates/server/tests/integration/` 测试夹具（具体形态在实现时确认，可能需要模拟 agent WS 连接）：

1. **成功路径**: 模拟 agent 发 Downloading → Verifying → PreFlight → Installing → Restarting → 断开 → 重连 + SystemInfo(new_version) → 断言 `status=Succeeded`
2. **失败路径**: 模拟 agent 发 Downloading → UpgradeResult(Verifying, "sha256 mismatch") → 断言 `status=Failed, stage=Verifying`
3. **超时路径**: 模拟 agent 发 Downloading → Restarting → 断开不再重连 → 快进 121s → 断言 `status=Timeout`
4. **并发拒绝**: 连续两次 POST /upgrade → 第二次 409
5. **target_version 错位**: POST 升 v1.0，agent 发 UpgradeProgress(target_version=v0.9) → 忽略，job 状态不变
6. **Agent 重连错版本**: POST 升 v1.0，agent 重连但 SystemInfo 仍是 v0.8 → Running 保持 → Timeout
7. **CAP_UPGRADE 关闭**: POST → 403，无 job 创建

### 前端测试（vitest，~8 tests）

- `use-upgrade-job.ts`: mock WS 消息，断言 state 转换（idle → running → succeeded / failed / timeout）
- `AgentVersionSection.test.tsx`:
  - idle 状态显示 "Upgrade to vX" 按钮
  - CAP_UPGRADE 关闭时按钮 disabled
  - Running 态显示 stepper，当前阶段高亮
  - Failed 态显示 Retry 按钮，点击触发 mutation
  - latest-version API 返回 null 时隐藏按钮 + 显示引导文案

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

**无新增环境变量**。现有 `SERVERBEE_RELEASE__BASE_URL`（或等价项，实现时确认）被新的 `GET /api/agent/latest-version` 复用。若该配置未设置，前端显示"Auto upgrade not configured"引导文案。

## 迁移

**无需数据库 migration**。所有新增状态在内存中追踪。

## 风险 & 未来工作

### 已知限制

1. **Windows 未验证**: 运行中的 exe 无法被 rename/delete；本设计不解决该遗留问题。
2. **服务端重启丢失活跃 job**: 内存态不持久化。可接受的降级：最终 `server.agent_version` 会被 SystemInfo 正常更新，但 UI 看不到升级确认。
3. **Timeout 后的"迟到成功"不翻转**: 如果 agent 在 120s 后才重连并上报新版本，UI 会同时看到"Timeout 状态"和"新版本号"。这是故意为之，避免引入"late success"概念。

### 可能的后续工作

- **批量升级**: 前端批量 POST 循环 + 进度聚合视图。
- **升级事件审计**: 新增 `event` 或 `agent_upgrade_log` 表，记录每次升级的触发者、时间、结果，供事后审计。
- **L2 Spawn-then-verify 回滚**: 新老二进制双向协议支持，彻底解决"能启动但跑不起来"的 corner case。本次不做因 ROI 不足。
- **Windows 升级验证 + 中继脚本方案**: 用 `MoveFileEx(MOVEFILE_DELAY_UNTIL_REBOOT)` 或外部 updater.exe 解决 Windows 自升级问题。
