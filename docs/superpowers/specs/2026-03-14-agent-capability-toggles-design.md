# Agent Capability Toggles — 设计文档

**日期**: 2026-03-14
**状态**: Draft

## 概述

为每台 Agent 提供 Per-Server 级别的远程功能开关，管理员可以针对每台服务器独立开启或关闭安全敏感的远程功能。采用双重校验机制（Server 端拦截 + Agent 端拒绝），确保即使 Server 被入侵，支持该协议的 Agent 也不会执行被禁用的功能。

## 需求

1. Per-Agent 级别的功能开关控制
2. 安全敏感功能默认关闭
3. Server 端 + Agent 端双重校验
4. 管理员通过 UI 管理开关（服务器详情页 + 独立设置页）
5. 开关变更通过 WebSocket 即时推送给 Agent

## 功能开关清单

| Bit | 常量 | 功能 | 危险等级 | 默认 |
|-----|------|------|---------|------|
| 0 | `CAP_TERMINAL` | Web 终端 (PTY) | ⚠️ 高 | 关闭 |
| 1 | `CAP_EXEC` | 远程命令执行 | ⚠️ 高 | 关闭 |
| 2 | `CAP_UPGRADE` | Agent 自动升级 | ⚠️ 高 | 关闭 |
| 3 | `CAP_PING_ICMP` | ICMP 探测 | 🟢 低 | 开启 |
| 4 | `CAP_PING_TCP` | TCP 探测 | 🟢 低 | 开启 |
| 5 | `CAP_PING_HTTP` | HTTP 探测 | 🟢 低 | 开启 |

默认值: `CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP` = `56` (0b0011_1000)

## 数据模型

### Capability Bitmap (common crate)

使用 `u32` 位图表示功能开关状态，每个 bit 对应一个功能。

```rust
pub const CAP_TERMINAL:  u32 = 1 << 0;  // 1
pub const CAP_EXEC:      u32 = 1 << 1;  // 2
pub const CAP_UPGRADE:   u32 = 1 << 2;  // 4
pub const CAP_PING_ICMP: u32 = 1 << 3;  // 8
pub const CAP_PING_TCP:  u32 = 1 << 4;  // 16
pub const CAP_PING_HTTP: u32 = 1 << 5;  // 32

pub const CAP_DEFAULT: u32 = CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP; // 56

// 有效 bit 掩码，用于校验输入
pub const CAP_VALID_MASK: u32 = 0b0011_1111; // 63
```

附带元数据结构体，用于前端展示和 OpenAPI 文档：

```rust
pub struct CapabilityMeta {
    pub bit: u32,
    pub key: &'static str,
    pub display_name: &'static str,
    pub default_enabled: bool,
    pub risk_level: &'static str, // "high" | "low"
}

pub const ALL_CAPABILITIES: &[CapabilityMeta] = &[
    CapabilityMeta { bit: CAP_TERMINAL,  key: "terminal",  display_name: "Web 终端",     default_enabled: false, risk_level: "high" },
    CapabilityMeta { bit: CAP_EXEC,      key: "exec",      display_name: "远程命令执行",  default_enabled: false, risk_level: "high" },
    CapabilityMeta { bit: CAP_UPGRADE,   key: "upgrade",   display_name: "自动升级",      default_enabled: false, risk_level: "high" },
    CapabilityMeta { bit: CAP_PING_ICMP, key: "ping_icmp", display_name: "ICMP 探测",    default_enabled: true,  risk_level: "low" },
    CapabilityMeta { bit: CAP_PING_TCP,  key: "ping_tcp",  display_name: "TCP 探测",     default_enabled: true,  risk_level: "low" },
    CapabilityMeta { bit: CAP_PING_HTTP, key: "ping_http", display_name: "HTTP 探测",    default_enabled: true,  risk_level: "low" },
];
```

### 数据库变更

`servers` 表新增一列：

```sql
ALTER TABLE servers ADD COLUMN capabilities INTEGER NOT NULL DEFAULT 56;
```

### 协议消息扩展

```rust
// ServerMessage 新增
ServerMessage::CapabilitiesSync { capabilities: u32 }

// Welcome 消息扩展 — 新增 capabilities 字段（向后兼容）
ServerMessage::Welcome {
    server_id: String,
    protocol_version: u32,  // 升至 2，表示支持 capability enforcement
    report_interval: u32,
    #[serde(default)]          // 向后兼容：旧 Server 不发此字段时默认 None
    capabilities: Option<u32>, // None 表示旧版 Server，Agent 按全部允许处理
}

// AgentMessage 新增 — Agent 拒绝时返回
AgentMessage::CapabilityDenied {
    msg_id: Option<String>,      // Exec 场景为 task_id
    session_id: Option<String>,  // Terminal 场景为 session_id
    capability: String,          // 被拒绝的 capability key
}
```

### 向后兼容与版本协商

**问题**: 当前 Agent 的消息解析使用 `serde_json::from_str::<ServerMessage>`，遇到未知 variant（如 `CapabilitiesSync`）会解析失败并静默丢弃。这意味着旧 Agent 无法识别新消息，Agent 侧的 capability enforcement 完全失效。

**解决方案**: 引入 `protocol_version` 协商机制：

1. **Server 端**: Welcome 中将 `protocol_version` 从 `1` 升至 `2`。Server 在 `AgentConnection` 中记录每个 Agent 报告的协议版本。

2. **Agent 端**: 新 Agent 在收到 Welcome 后，发送 `AgentMessage::SystemInfo` 时携带 `protocol_version: 2` 字段（`#[serde(default)]`，旧 Agent 不发此字段默认为 1）。新 Agent 能正确解析 `CapabilitiesSync` 并执行 enforcement。

3. **Server 端感知 Agent 版本**: Server 从 Agent 的 SystemInfo 中读取 `protocol_version`：
   - `protocol_version >= 2`: Agent 支持 capability enforcement，双重校验生效
   - `protocol_version == 1`（或缺失）: 旧 Agent，**仅 Server 端拦截生效**

4. **Server 端行为差异**: 对旧 Agent，Server 不发送 `CapabilitiesSync`（避免解析错误日志）；所有 capability 检查仍在 Server 端执行。

5. **持久化 Agent 协议版本**: Server 收到 SystemInfo 后，除了更新内存中 `AgentConnection.protocol_version`，还需将 `protocol_version` 持久化到 `servers` 表（新增 `protocol_version INTEGER` 列，默认 1）。这样：
   - REST API `ServerResponse` 可以返回 `protocol_version` 字段
   - 离线 Agent 也能在 Dashboard 上标注版本信息
   - Browser WS 的 `FullSync`/`ServerOnline` 可以携带该信息

6. **前端数据模型**:
   - `ServerResponse`（REST）增加 `protocol_version: number` 字段
   - `ServerStatus`（Browser WS FullSync）增加 `protocol_version: Option<i32>`（FullSync 时从 DB 填充，Update 时为 None 不覆盖）
   - 前端根据 `protocol_version < 2` 显示旧版 Agent 警告

**安全声明修正**: 双重校验仅在 Agent 版本 >= 2 时完整生效。旧版 Agent 仅有 Server 端单层防护。Dashboard 根据持久化的 `protocol_version` 提示管理员升级旧 Agent。

## 数据流

### 初始同步

```
Agent 连接 WebSocket
  → Server 发送 Welcome { ..., protocol_version: 2, capabilities: Some(56) }
  → Agent 将 capabilities 存入 AtomicU32
  → 若 capabilities 为 None（旧版 Server），Agent 设为 u32::MAX（全部允许）
Agent 发送 SystemInfo { ..., protocol_version: 2 }
  → Server 记录该 Agent 的 protocol_version 到 AgentConnection
```

### 运行时变更

```
管理员修改开关 → PUT /api/servers/:id { capabilities }
  → Server 校验: capabilities & ~CAP_VALID_MASK == 0，否则 422
  → Server 更新 DB
  → 若 Agent 在线且 protocol_version >= 2:
      → Server 通过 Agent WS 发送 CapabilitiesSync { capabilities }
      → Agent 更新 AtomicU32，立即生效
  → 若 Agent 在线但 protocol_version == 1:
      → 不发送 CapabilitiesSync（旧 Agent 无法解析）
      → Server 端拦截仍然生效
  → Server 通过 Browser broadcast 发送 CapabilitiesChanged { server_id, capabilities }
  → 前端更新 UI（按钮状态、Toggle 开关）
```

### capabilities 变更后触发 Ping 重新同步

```
管理员修改 capabilities（含 CAP_PING_* 位变更）
  → Server 更新 DB + 推送 CapabilitiesSync（如上）
  → Server 调用 PingService::sync_tasks_to_agent(db, agent_manager, server_id)
  → 该方法按新的 capabilities 过滤后生成任务列表（可能为空列表）
  → 发送 PingTasksSync { tasks: [...] } 给 Agent
  → Agent 用收到的列表全量替换当前 ping 任务（空列表 = 全部停止）
```

### Agent 离线时

Agent 重连后通过 Welcome 消息获取最新 capabilities，无需额外同步机制。

## 双重校验

### Server 端拦截

在派发任务前检查目标 Agent 的 capabilities：

| 操作 | 检查点 | 未启用时行为 |
|------|--------|------------|
| 终端请求 | terminal.rs WS handler | 返回 403 "Terminal is disabled for this server" |
| 命令执行 | POST /api/tasks handler | 过滤掉未启用的服务器；对被跳过的服务器写入 synthetic task_result（见下文） |
| Agent 升级 | POST /api/servers/:id/upgrade | 返回 403 "Upgrade is disabled for this server" |
| Ping 任务同步 | PingTasksSync 生成时 | 在现有 server_ids 过滤之上，再按 probe_type 检查对应 CAP_PING_* 位，两个条件都满足才同步；过滤后为空列表时**仍然发送** `PingTasksSync { tasks: [] }`，确保 Agent 停止所有该类型探测 |

### Server 端 Exec 跳过处理

当 POST /api/tasks 中的部分或全部目标服务器因 capability 被拦截时：

1. 对每台被拦截的服务器，**立即写入 synthetic `task_result` 记录**：
   - `output`: `"Capability 'exec' is disabled for this server"`
   - `exit_code`: `-2`（区别于 Agent 拒绝的 `-1`）
   - `finished_at`: 当前时间
2. API 响应正常返回 task_id，前端照常轮询结果
3. 前端显示 synthetic result 时用不同样式（如灰色文字 + 禁用图标）标注「已跳过」

这样前端无需特殊逻辑处理「部分服务器消失」的情况。

### Agent 端拒绝

Agent 内存中维护 `capabilities: AtomicU32`，收到请求时校验：

| 收到消息 | 检查 | 未启用时行为 |
|---------|------|------------|
| `Exec` | `CAP_EXEC` | 发送 `CapabilityDenied { msg_id: Some(task_id), capability: "exec" }`，写 warn 日志，不执行 |
| `TerminalOpen` | `CAP_TERMINAL` | 发送 `CapabilityDenied { session_id: Some(session_id), capability: "terminal" }`，不创建 PTY |
| `Upgrade` | `CAP_UPGRADE` | 发送 `CapabilityDenied { msg_id: None, capability: "upgrade" }`，不下载/替换 |
| `PingTasksSync` | 各 `CAP_PING_*` | 过滤掉不允许的 probe_type，只执行被允许的（不发送 CapabilityDenied，静默过滤） |

### CapabilityDenied 处理

Server 收到 `CapabilityDenied` 后：

- 写 warn 日志（Server/Agent 状态不同步的信号）
- Exec：根据 `msg_id` (task_id) 在 `task_result` 中记录 `"Capability denied by agent"` + `exit_code = -1`
- Terminal：根据 `session_id` 关闭对应 Browser WS 连接，前端显示 "此服务器已禁用终端功能"

### 在途操作处理

当功能开关被禁用时，已经在执行的操作行为：

- **终端**：已建立的 PTY 会话继续运行直到自然关闭，但不允许新建会话
- **命令执行**：已在执行的命令继续运行直到完成/超时，但不允许新任务
- **Ping 探测**：capabilities 变更后立即触发 `sync_tasks_to_agent`，Agent 收到新的（可能为空的）任务列表后全量替换，被禁用的探测类型即刻停止

## API 变更

### 现有 API 扩展

**`PUT /api/servers/:id`** — UpdateServerInput 增加可选字段：

```json
{ "capabilities": 56 }
```

校验规则：
- `capabilities & ~CAP_VALID_MASK != 0` 时返回 422（包含未定义的 bit 位）
- 仅 admin 可修改 capabilities

**`GET /api/servers`** 和 **`GET /api/servers/:id`** — 响应增加字段：

```json
{
  "id": "xxx",
  "name": "prod-web-01",
  "capabilities": 56
}
```

capabilities 字段对所有认证用户可见（admin + member）。理由：capabilities 是服务器的运行状态信息，非 admin 用户知道某台服务器是否开启了终端不构成安全风险（他们本身就没有权限使用这些功能）。如需隐藏，可以在 member 的响应中省略此字段，但增加了复杂度且收益有限。

**Browser WebSocket** — 新增 `CapabilitiesChanged` 消息类型：

```rust
BrowserMessage::CapabilitiesChanged {
    server_id: String,
    capabilities: u32,
}
```

**运行时 capabilities 变更**通过独立的 `CapabilitiesChanged` 消息推送。

**`ServerStatus` 不新增 capabilities 字段**。理由：
- `ServerStatus` 被 `FullSync` 和 `Update` 共用。`update_report` 构建 `Update` 时没有 DB 访问，无法获取 capabilities
- 如果加为必填字段，每 3 秒的 metric update 都需填充，填 0 或旧值会导致前端 merge 覆盖正确值
- capabilities 变更频率极低（管理员偶尔操作），不应混入高频 metric update

**前端 capabilities 数据来源**（三条独立路径，无矛盾）：
1. **REST API**: `GET /api/servers` 和 `GET /api/servers/:id` 响应包含 `capabilities` 字段 → 填充 query cache `['servers-list']` 和 `['servers', id]`
2. **Browser WS FullSync**: 浏览器连接时不依赖 WS 获取 capabilities，由 REST query 提供初始值
3. **Browser WS CapabilitiesChanged**: 管理员修改后推送 → 前端 handler 需要同时 invalidate 或直接更新所有相关 query key：
   - `['servers']`（Dashboard WS 缓存）
   - `['servers', serverId]`（详情页）
   - `['servers-list']`（任务页等列表页）

前端 `use-servers-ws.ts` 新增 `capabilities_changed` 消息处理：

```typescript
// use-servers-ws.ts 新增 handler
case 'capabilities_changed': {
  const { server_id, capabilities } = msg
  // 更新 Dashboard WS 缓存
  queryClient.setQueryData(['servers'], (prev) =>
    prev?.map(s => s.id === server_id ? { ...s, capabilities } : s)
  )
  // 更新详情页缓存
  queryClient.setQueryData(['servers', server_id], (prev) =>
    prev ? { ...prev, capabilities } : prev
  )
  // 失效列表页缓存（触发 refetch）
  queryClient.invalidateQueries({ queryKey: ['servers-list'] })
  break
}
```

### 新增 API

**`PUT /api/servers/batch-capabilities`** (admin only)

批量更新功能开关，使用位运算避免竞态：

```json
// Request
{
  "server_ids": ["id1", "id2", "id3"],
  "set": 1,
  "unset": 2
}

// Response
{ "data": { "updated": 3 } }
```

校验规则：
- `set` 和 `unset` 必须都在 `CAP_VALID_MASK` 范围内，否则 422
- `set & unset != 0` 时返回 422（同一 bit 不能同时 set 和 unset）
- 执行顺序：`capabilities = (capabilities & ~unset) | set`

对每台受影响的在线 Agent 发送 `CapabilitiesSync`（仅 protocol_version >= 2 的），对 Browser 广播 `CapabilitiesChanged`。如果变更涉及 `CAP_PING_*` 位，同时触发 `sync_tasks_to_agent` 重新同步 ping 任务。

### 单服务器 PUT 的 capabilities 更新

`PUT /api/servers/:id` 中的 capabilities 字段是完整替换语义（last-write-wins）。由于单台服务器的开关通常只有一个管理员在操作，竞态风险可接受。如需原子操作，使用 batch API 的 set/unset 语义。

## 审计日志

capabilities 变更记入 `audit_log` 表：

| 字段 | 值 |
|------|---|
| `action` | `"capabilities_changed"` |
| `detail` | `{"server_id": "xxx", "old": 56, "new": 57, "changed_bits": {"terminal": "enabled"}}` |
| `user_id` | 操作者 ID |
| `ip` | 请求 IP |

批量操作时，为每台受影响的服务器生成一条独立的审计记录。

## 前端 UI

### 1. 服务器详情页 — 功能开关 Section

在 `/servers/:id` 详情页增加 "功能开关" 卡片，位于服务器元数据下方：

- 每行：功能名称 + Toggle 开关 + 危险等级标签
- 高危功能旁显示警告色标签
- Toggle 变更后立即调用 `PUT /api/servers/:id` 更新
- 仅 admin 可见
- 如果 Agent 版本为旧版（protocol_version < 2），显示提示：「Agent 版本不支持功能开关强制执行，请升级 Agent」

### 2. 独立设置页 — `/settings/capabilities`

左侧导航新增 "功能开关" 入口，表格展示所有服务器的开关矩阵：

- 表格列：服务器名称 + 每个 capability 的 Toggle
- 支持搜索/排序
- 多选后批量修改（全部开启/关闭某功能、重置为默认）
- 调用 `PUT /api/servers/batch-capabilities` 批量更新
- 仅 admin 可访问
- 旧版 Agent 的行用不同样式标注（如浅色背景 + 警告图标）

### 3. 功能禁用时的交互反馈

- **终端按钮**：`CAP_TERMINAL` 关闭时，"打开终端" 按钮 disabled + tooltip "终端功能已禁用"
- **命令执行**：Tasks 页面中，`CAP_EXEC` 关闭的服务器显示灰色 + 提示
- **升级按钮**：`CAP_UPGRADE` 关闭的服务器不显示升级操作
- **任务结果**：exit_code 为 -2 的 synthetic result 显示为「已跳过 — 功能已禁用」样式

## 安全考虑

1. **默认安全**：高危功能（终端、命令执行、自动升级）默认关闭，必须管理员显式启用
2. **双重校验**：在 Agent protocol_version >= 2 时完整生效；旧版 Agent 仅有 Server 端单层防护
3. **Admin Only**：功能开关的修改权限仅限 admin 角色
4. **审计日志**：所有 capabilities 变更记入 audit_log，含旧值/新值/变更位
5. **即时生效**：开关变更通过 WS 推送立即同步到 Agent
6. **输入校验**：拒绝 `CAP_VALID_MASK` 范围外的 bit 位，防止未定义位被设置
7. **版本感知**：Server 区分新旧 Agent，Dashboard 提示管理员升级旧版 Agent 以获得完整保护
8. **capabilities 可见性**：所有认证用户可读取 capabilities 状态（不构成安全风险），仅 admin 可修改

## 测试策略

### 单元测试

- capabilities 位运算辅助函数
- Server 端各拦截点的 capability 检查逻辑
- Agent 端各消息处理的 capability 校验逻辑
- 输入校验：未知 bit 位拒绝、set/unset 重叠拒绝
- Exec synthetic task_result 生成逻辑

### 集成测试

- 修改 capabilities → 验证 WS 推送 → 验证 Agent 收到新值
- 禁用终端 → 尝试打开终端 → 验证 Server 返回 403
- 禁用命令执行 → Agent 收到 Exec → 验证 CapabilityDenied 返回
- Welcome 向后兼容：无 capabilities 字段时 Agent 按全部允许处理
- Ping capability 变更 → 验证空列表同步 → 验证 Agent 停止探测
- 旧 Agent（protocol_version 1）→ Server 不发送 CapabilitiesSync → Server 端拦截仍生效
- POST /api/tasks 部分目标被拦截 → 验证 synthetic task_result 写入 + 前端正常展示

### 前端测试

- Toggle 开关交互 → API 调用正确
- 禁用状态下按钮 disabled
- 批量操作功能正确
- CapabilitiesChanged WS 消息 → UI 实时更新
- Synthetic task result（exit_code -2）→ 正确展示「已跳过」样式
- 旧版 Agent 提示标注
