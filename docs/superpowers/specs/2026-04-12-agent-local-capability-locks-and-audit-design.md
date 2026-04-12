# Agent Local Capability Locks And High-Risk Audit — 设计文档

**日期**: 2026-04-12
**状态**: Draft

## 概述

现有能力开关只由服务端控制，安全边界仍然偏单侧。管理员可以在服务端关闭高风险能力，但一旦服务端被误操作或被入侵，Agent 没有自己的硬底线。

本次设计补两块：

1. Agent 增加本地能力策略，作为不可被服务端提升的本地上限
2. 补齐高风险用户操作审计，尤其是敏感读取和会话型访问

目标不是引入复杂策略系统，而是把能力控制收紧成一个清晰、可解释、可审计的模型。

## 目标

1. 支持 Agent 通过 CLI 显式允许或永久关闭能力
2. 服务端只能在 Agent 本地允许的前提下开启能力，不能突破 Agent 本地上限
3. UI 能明确显示“客户端关闭”，并禁用对应服务端开关
4. 为高风险用户操作补齐成功和拒绝审计
5. 审计只记录用户操作，不记录程序内部同步和后台任务

## 非目标

1. 不引入 OPA、ABAC、审批流、JIT 临时授权
2. 不重做现有 capability bitmask 模型
3. 不记录终端内容或 Docker 日志内容
4. 不为旧 Agent 单独设计兼容 UX

## 部署前提

本设计按“无历史部署兼容负担”处理。

首版默认策略直接采用：

- 低风险默认开启
- 高风险默认关闭

不为了兼容既有 Agent 部署而把本地能力默认值放宽为 `u32::MAX`。

## 关键决策

### 决策 1: Agent 本地能力策略使用 CLI 参数

Agent 增加重复参数：

- `--allow-cap <name>`
- `--deny-cap <name>`

能力名使用现有 capability key：

- `terminal`
- `exec`
- `upgrade`
- `ping_icmp`
- `ping_tcp`
- `ping_http`
- `file`
- `docker`

默认策略：

- 低风险默认开启: `ping_icmp`、`ping_tcp`、`ping_http`
- 高风险默认关闭: `terminal`、`exec`、`upgrade`、`file`、`docker`

冲突处理：

- `deny` 优先于 `allow`

这意味着：

- `--allow-cap exec --deny-cap exec` 的最终结果是关闭 `exec`
- 如果用户完全不传参数，Agent 仍保持当前安全默认行为

### 决策 2: 最终生效能力采用双层交集

系统内部区分 3 个能力集合：

- `server_caps`: 服务端数据库中配置的能力位
- `agent_local_caps`: Agent 本地根据默认值与 CLI 计算出的能力位
- `effective_caps`: 最终实际生效的能力位

计算规则固定为：

`effective_caps = server_caps & agent_local_caps`

结论：

- 服务端只能收紧，不能放大 Agent 本地能力
- Agent 本地关闭的能力，服务端 UI 不能再把它打开

### 决策 3: UI 以“实际能不能用”为主真相

当某个能力被 Agent 本地策略永久关闭时：

- 服务端 UI 中对应开关直接 disabled
- 鼠标 hover 显示 `客户端关闭`

页面仍保留服务端配置位，但主交互以运行时实际可用性为准，避免出现“看起来打开了，实际永远用不了”的假状态。

### 决策 4: 审计只记录用户操作

审计范围仅包含用户发起的高风险访问，不包含：

- Agent 自身上报
- WebSocket relay
- 后台同步任务
- 内部缓存刷新
- 系统自动事件

“用户操作”包括：

- 浏览器 session
- Bearer token
- API key

### 决策 5: 成功和拒绝都审计

所有纳入范围的高风险操作，成功和拒绝都要写审计日志。

拒绝原因必须结构化，不允许继续用“capability disabled”这种模糊字符串做全部语义。

## Agent 侧设计

### CapabilityKey 类型

新增 `CapabilityKey` 枚举，避免 CLI、协议和审计层继续使用裸字符串：

- `Terminal`
- `Exec`
- `Upgrade`
- `PingIcmp`
- `PingTcp`
- `PingHttp`
- `File`
- `Docker`

该枚举提供：

- `FromStr`，用于 CLI 解析
- `to_bit() -> u32`，用于转回 bitmask
- `as_str() -> &'static str`，用于协议与审计输出

### AgentConfig 扩展

Agent 增加本地能力策略配置，但来源以 CLI 为主。配置结构上建议新增运行时字段：

- `allow_caps: Vec<CapabilityKey>`
- `deny_caps: Vec<CapabilityKey>`

### CLI 解析

在 Agent 启动入口解析重复参数：

```text
serverbee-agent --allow-cap terminal --allow-cap exec --deny-cap ping_http
```

解析后生成 `agent_local_caps`。

### CLI 解析机制

Agent 当前使用 Figment 负责 TOML 与环境变量加载，不负责 repeatable CLI 参数解析。

本次不引入 `clap`。首版使用一个小型自定义解析器基于 `std::env::args()` 处理：

- `--allow-cap <name>`
- `--deny-cap <name>`

职责划分：

- Figment: `agent.toml` + `SERVERBEE_*` 环境变量
- 自定义 CLI 解析: 本地能力策略参数

这样能在不扩大依赖面的前提下满足当前需求。

### 本地能力计算

基准值从默认能力开始：

- 默认置入 `CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP`

再按顺序应用：

1. 对 `allow_caps` 置位
2. 对 `deny_caps` 清位

由于 `deny` 优先，最终状态以清位结果为准。

### Agent 执行判断

Agent 内所有 capability 判断改用 `effective_caps`，不再直接使用服务端同步下来的值。

受影响路径包括：

- terminal
- exec
- upgrade
- file
- docker
- ping/probe

也就是说，即使服务端把某能力打开，只要该能力不在 `agent_local_caps` 内，Agent 仍然拒绝执行。

## 协议与状态同步

### Agent -> Server 上报本地能力

Agent 连接后发送的 `AgentMessage::SystemInfo` 变体增加一个新字段：

- `agent_local_capabilities: Option<u32>`

结构示意：

```rust
AgentMessage::SystemInfo {
    msg_id: String,
    info: SystemInfo,
    #[serde(default)]
    agent_local_capabilities: Option<u32>,
}
```

该字段放在消息变体本身，而不是 `SystemInfo` 结构体内。

原因：

1. `SystemInfo` 当前语义是硬件与系统信息，不应混入运行时访问策略
2. collector 不需要感知 capability 概念
3. `#[serde(default)]` 可以保持向后兼容

### Server 运行时状态

Server 为每台在线 Agent 维护：

- `server_caps`
- `agent_local_caps`
- `effective_caps`

其中：

- `server_caps` 来自 DB
- `agent_local_caps` 来自最新 `AgentMessage::SystemInfo.agent_local_capabilities`
- `effective_caps` 为实时计算结果

`agent_local_caps` 首版只存在内存态，不写入数据库。

原因：

1. 它是 Agent 当前进程的运行时事实，不是服务端配置
2. UI 的 `disabled` 需求只要求在线时准确
3. 避免在本次范围内扩大 DB/migration 改动

如果未来要在离线时展示最近一次本地策略，再考虑持久化。

### BrowserMessage 扩展

`CapabilitiesChanged` 需从单字段扩展为完整运行时视图：

- `server_id`
- `capabilities`
- `agent_local_capabilities`
- `effective_capabilities`

否则能力设置页和服务器详情页无法在 WS 增量更新时正确刷新禁用状态。

## Server API 设计

### ServerResponse 扩展

`GET /api/servers`
`GET /api/servers/{id}`

响应新增字段：

- `agent_local_capabilities?: number | null`
- `effective_capabilities?: number | null`

语义：

- 在线且已上报本地能力时，这两个字段有值
- 离线或尚未上报时，这两个字段为 `null`

### 运行时计算规则

API 返回时：

- `capabilities` 继续表示服务端配置位
- `agent_local_capabilities` 表示 Agent 本地上限
- `effective_capabilities` 表示运行时实际生效位

当 `agent_local_capabilities == null` 时，不做本地锁定判断。

### 服务端能力修改语义

管理员修改 capability 时，仍然只写服务端配置位。

服务端不尝试：

- 修改 Agent 本地策略
- 覆盖 Agent `deny`
- 自动修正本地 CLI 配置

服务端能力修改的效果是：

- 更新 `server_caps`
- 更新 `effective_caps`
- 向浏览器广播最新运行时状态
- 向 Agent 下发新的服务端位图，由 Agent 本地重新求交集

## UI 设计

### 能力开关页与详情页

前端统一使用 3 组状态：

- `server_enabled`
- `client_enabled`
- `effective_enabled`

判断逻辑：

- `server_enabled = hasCap(capabilities, bit)`
- `client_enabled = agent_local_capabilities == null ? true : hasCap(agent_local_capabilities, bit)`
- `effective_enabled = effective_capabilities == null ? server_enabled : hasCap(effective_capabilities, bit)`

### 交互规则

#### 情况 A: 客户端关闭

如果 `client_enabled = false`：

- 开关 disabled
- hover tooltip 显示 `客户端关闭`
- 视觉状态以 `effective_enabled = false` 展示

#### 情况 B: 客户端允许

如果 `client_enabled = true`：

- 开关保持可操作
- 操作仍只修改服务端配置位

### 批量操作

批量能力切换保持现有服务端配置更新能力，但单个单元格在客户端关闭时直接禁用。

结果：

- 用户可以看见哪些机器被客户端锁死
- 用户不会误以为自己真的打开了这些能力

## 错误语义与拒绝回传

### AgentMessage::CapabilityDenied 扩展

现有 `CapabilityDenied` 信息不足，无法区分拒绝来源。

增加字段：

- `capability`
- `reason`

完整语义：

- `capability`: 哪个能力被拒绝，例如 `exec`
- `reason`: 为什么被拒绝

`reason` 首版仅包括：

- `server_capability_disabled`
- `agent_capability_disabled`

### HTTP 错误映射

对于文件、Docker、终端等服务端入口，错误响应不再统一为模糊字符串。

与 capability 相关的拒绝原因收敛为：

- `server_capability_disabled`
- `agent_capability_disabled`

对 UI 展示而言：

- 服务器配置关闭，可展示为 `服务端已禁用该能力`
- 客户端锁死，可展示为 `客户端关闭`

其他错误继续沿用现有 `AppError` 体系，例如：

- offline
- not found
- ownership mismatch
- timeout

本次不把所有错误都折叠进 capability reason 枚举。

## 审计设计

### 审计存储策略

复用现有 `audit_logs` 表：

- `action` 继续表示事件类型
- `detail` 改为结构化 JSON 字符串

本次不新增审计表字段，不做 migration。

原因：

1. 当前表结构足够容纳本次范围
2. 结构化 detail 已能支持筛选和回放
3. 可将 schema 升级延后到后续专门的审计治理工作

### 记录范围

#### Terminal

记录：

- 谁打开了 terminal
- 哪台服务器
- session_id
- started_at
- ended_at
- duration_ms
- close_reason
- IP

不记录：

- 输入内容
- 输出内容

建议 action：

- `terminal_opened`
- `terminal_open_denied`
- `terminal_closed`

#### Exec

记录：

- 谁执行了命令
- 哪台服务器
- task_id
- 完整命令文本
- timeout
- exit_code
- 拒绝原因
- IP

建议 action：

- `exec_started`
- `exec_denied`
- `exec_finished`

#### File

现有文件变更审计继续保留并统一为 JSON detail：

- `file_write`
- `file_delete`
- `file_mkdir`
- `file_move`
- `file_upload`

本次新增重点是补齐缺失的读取侧与拒绝侧审计。

读取侧只记录真正读取内容或导出的行为：

- `read`
- `download`

不记录：

- `list`
- `stat`

建议 action：

- `file_read`
- `file_read_denied`
- `file_download`
- `file_download_denied`

detail 中包含：

- `server_id`
- `path`
- `transfer_id`（download）
- `deny_reason`

#### Docker REST 读取

记录用户访问以下资源：

- containers
- stats
- info
- events
- networks
- volumes

建议 action：

- `docker_view`
- `docker_view_denied`

detail 中包含：

- `server_id`
- `resource`
- `deny_reason`

#### Docker Logs WebSocket

记录：

- 谁订阅了哪个 container 的日志
- session_id
- started_at
- ended_at
- duration_ms
- close_reason
- follow
- tail
- IP

不记录：

- 实际日志内容

建议 action：

- `docker_logs_subscribed`
- `docker_logs_subscribe_denied`
- `docker_logs_unsubscribed`

### 审计 detail JSON 约束

所有 detail 统一写 JSON，不允许继续写自然语言描述句子。

示例：

```json
{
  "server_id": "srv_123",
  "path": "/var/log/nginx/access.log",
  "deny_reason": "agent_capability_disabled"
}
```

这保证后续：

- 列表页可筛选
- 导出日志时可机器处理
- 安全分析不需要解析拼接字符串

## 入口挂点设计

### Terminal

审计挂在服务端 WebSocket handler 生命周期：

1. 握手成功并准备打开 session 时写 `terminal_opened`
2. 因权限或 capability 被拒绝时写 `terminal_open_denied`
3. 会话结束时写 `terminal_closed`

`terminal_closed` 的 `close_reason` 候选值：

- `client_closed`
- `idle_timeout`
- `server_disconnect`
- `agent_disconnect`
- `open_failed`

为支持 `started_at`、`ended_at`、`duration_ms`，需要维护单独的 terminal 审计上下文，例如：

- `terminal_audit_contexts: DashMap<String, TerminalAuditContext>`

其中 `TerminalAuditContext` 至少包含：

- `server_id`
- `user_id`
- `ip`
- `started_at`

这样不会把现有 `terminal_sessions` 的输出路由职责和审计生命周期职责揉在一起。

### Exec

审计挂在服务端任务派发路径：

1. 即将向 Agent 派发时写 `exec_started`
2. 服务端入口拦截时写 `exec_denied`
3. Agent 结果回传并持久化时写 `exec_finished`

### File Read / Download

#### read

在 HTTP handler 内：

1. 入口拦截失败写 `file_read_denied`
2. Agent 返回成功内容后写 `file_read`

#### download

下载是两段式：

1. 发起下载任务
2. 通过 transfer_id 取流

首版建议以真正开始流式下载为准写 `file_download`，因为这是实际导出内容的时刻。

若在下载入口因权限、能力或所有权失败，则写 `file_download_denied`。

### Docker REST 读取

在各个 HTTP GET handler 内：

1. 入口拦截失败写 `docker_view_denied`
2. 成功返回数据时写 `docker_view`

`resource` 为固定枚举值，不使用自由文本。

### Docker Logs

在 WebSocket 生命周期内：

1. 用户发起订阅且通过校验时写 `docker_logs_subscribed`
2. 被拒绝时写 `docker_logs_subscribe_denied`
3. 会话结束时写 `docker_logs_unsubscribed`

与 terminal 同理，Docker logs 需要独立审计上下文，例如：

- `docker_logs_audit_contexts: DashMap<String, DockerLogsAuditContext>`

其中至少包含：

- `server_id`
- `user_id`
- `ip`
- `container_id`
- `tail`
- `follow`
- `started_at`

## 测试策略

### 1. Agent 单元测试

覆盖：

1. 默认低风险开、高风险关
2. `--allow-cap` 打开默认关闭的能力
3. `--deny-cap` 关闭默认开启的能力
4. `deny` 覆盖 `allow`
5. `effective_caps = server_caps & agent_local_caps`

### 2. Agent 行为测试

覆盖：

1. 本地禁用 `exec` 时，服务端开启也拒绝执行
2. 本地禁用 `terminal` 时，终端打开请求被拒绝
3. 本地禁用 `file` 时，文件读请求被拒绝
4. 本地禁用 `docker` 时，Docker 请求被拒绝
5. 返回的拒绝 reason 正确

### 3. Server 集成测试

覆盖：

1. Agent 上报 `agent_local_capabilities`
2. `/api/servers` 与 `/api/servers/{id}` 返回新增字段
3. 服务端能力变更后 `effective_capabilities` 正确变化
4. `BrowserMessage::CapabilitiesChanged` 包含完整运行时字段
5. 客户端关闭能力时，对应操作入口返回正确拒绝 reason

### 4. 审计测试

覆盖：

1. Terminal 成功/拒绝/关闭都写日志
2. Exec 成功/拒绝/完成都写日志，且命令文本完整
3. File read/download 成功和拒绝都写日志
4. Docker REST 读取成功和拒绝都写日志
5. Docker logs 订阅与结束都写日志
6. detail 为 JSON，可稳定解析
7. 非用户操作不写审计

### 5. 前端测试

覆盖：

1. 当 `agent_local_capabilities` 关闭某能力时，开关 disabled
2. hover 显示 `客户端关闭`
3. `effective_capabilities` 决定最终状态文案
4. WS 增量更新后页面正确刷新

## 风险与取舍

### 风险 1: 运行时状态不持久化

首版 `agent_local_capabilities` 只在内存中维护。

影响：

- Agent 离线后 UI 无法继续显示最近一次客户端锁定状态

取舍理由：

- 本次核心目标是在线时的真实控制与真实展示
- 持久化会扩大 migration 和状态一致性复杂度

### 风险 2: 审计表仍是通用表

结构化 detail 虽然够用，但查询体验不会像专用审计表那样舒服。

取舍理由：

- 先把事实记全
- 表结构治理可以在后续独立做

### 风险 3: Exec 审计可能包含敏感参数

按产品要求，审计记录完整命令文本。

边界条件：

- 命令长度受现有 `MAX_COMMAND_SIZE` 限制
- 本次不再额外引入第二套截断规则，避免前后语义不一致

这意味着审计系统需要接受命令文本可能包含敏感参数这一事实。

## 实现顺序建议

1. Agent CLI 与本地能力计算
2. Agent 执行判断切换到 `effective_caps`
3. Agent `SystemInfo` 上报 `agent_local_capabilities`
4. Server 运行时状态、API、BrowserMessage 扩展
5. UI disabled + tooltip
6. 拒绝 reason 结构化
7. 审计补齐
8. 测试补齐

## 结论

本设计把能力控制从“只有服务端决定”收紧为“服务端配置 + Agent 本地硬上限”的双层模型，同时为高风险访问补齐结构化审计。

能力边界会变得更清楚：

- 服务端负责集中管理
- Agent 负责本地最终执行门槛
- UI 负责诚实展示当前是否真的可用
- 审计负责回答谁在什么时候做了什么，或者试图做什么

这套方案不复杂，但足够硬，能把当前模型里最容易骗人的地方先掰正。
