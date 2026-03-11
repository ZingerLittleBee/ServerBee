# ServerBee VPS 探针 - 架构设计文档

> **服务端**: Rust (Axum + sea-orm + tokio)
> **Agent**: Rust (共享 common crate)
> **前端**: React 19+ SPA (Vite + TW4 + shadcn/ui + TanStack Router)
> **数据库**: SQLite
> **通信**: WebSocket (JSON)
> **部署**: 单二进制 + Docker + install.sh

---

## 1. 架构总览

```
┌─────────────────────────────────────────────┐
│            ServerBee Dashboard               │
│                                             │
│  ┌─────────────────────────────────────┐    │
│  │ 前端 (React SPA, rust-embed 嵌入)   │    │
│  │ React 19+ / TanStack Router/Query  │    │
│  │ shadcn/ui / Tailwind CSS v4        │    │
│  └─────────────────────────────────────┘    │
│  ┌─────────────────────────────────────┐    │
│  │ 服务端 (Rust)                       │    │
│  │ Axum Router                        │    │
│  │  ├── REST API handlers             │    │
│  │  ├── WebSocket: Agent + Browser    │    │
│  │  └── 静态文件 (rust-embed)          │    │
│  │ Service Layer                      │    │
│  │  ├── AgentManager (连接/状态)       │    │
│  │  ├── AlertService (告警评估)        │    │
│  │  ├── RecordService (指标/聚合)      │    │
│  │  └── NotificationService (通知)    │    │
│  │ Entity Layer (sea-orm)             │    │
│  │  └── SQLite                        │    │
│  └─────────────────────────────────────┘    │
└──────────────┬──────────────────────────────┘
               │ WebSocket
┌──────────────▼──────────────────────────────┐
│           ServerBee Agent (Rust)             │
│  common crate 共享类型/协议                  │
│  ├── Collector (系统指标采集)                │
│  ├── Reporter (WebSocket 上报+重连)          │
│  ├── Probe (ICMP/TCP/HTTP 探测)             │
│  ├── Executor (远程命令)                    │
│  └── Terminal (PTY 终端)                    │
└─────────────────────────────────────────────┘
```

### Cargo Workspace 结构

```
serverbee/
├── Cargo.toml                # workspace
├── crates/
│   ├── common/               # 共享: 协议定义、数据类型、序列化
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── protocol.rs   # Agent <-> Server 消息类型
│   │       ├── types.rs      # SystemReport, GpuInfo 等
│   │       └── constants.rs  # 版本号、默认值
│   ├── server/               # 服务端
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config.rs
│   │       ├── state.rs          # AppState
│   │       ├── router/
│   │       │   ├── mod.rs
│   │       │   ├── api/          # REST API handlers
│   │       │   │   ├── auth.rs
│   │       │   │   ├── server.rs
│   │       │   │   ├── alert.rs
│   │       │   │   ├── notification.rs
│   │       │   │   ├── ping.rs
│   │       │   │   ├── task.rs
│   │       │   │   └── setting.rs
│   │       │   ├── ws/           # WebSocket handlers
│   │       │   │   ├── agent.rs
│   │       │   │   ├── browser.rs
│   │       │   │   └── terminal.rs
│   │       │   └── static_files.rs
│   │       ├── service/
│   │       │   ├── mod.rs
│   │       │   ├── agent_manager.rs
│   │       │   ├── auth.rs
│   │       │   ├── record.rs
│   │       │   ├── alert.rs
│   │       │   ├── notification.rs
│   │       │   ├── ping.rs
│   │       │   ├── task.rs
│   │       │   ├── config.rs
│   │       │   ├── geoip.rs
│   │       │   └── user.rs
│   │       ├── entity/           # sea-orm Entity
│   │       │   ├── mod.rs
│   │       │   ├── user.rs
│   │       │   ├── session.rs
│   │       │   ├── api_key.rs
│   │       │   ├── server.rs
│   │       │   ├── server_group.rs
│   │       │   ├── server_tag.rs
│   │       │   ├── record.rs
│   │       │   ├── record_hourly.rs
│   │       │   ├── gpu_record.rs
│   │       │   ├── alert_rule.rs
│   │       │   ├── notification.rs
│   │       │   ├── notification_group.rs
│   │       │   ├── ping_task.rs
│   │       │   ├── ping_record.rs
│   │       │   ├── task.rs
│   │       │   ├── task_result.rs
│   │       │   ├── config.rs
│   │       │   ├── alert_state.rs
│   │       │   └── audit_log.rs
│   │       ├── migration/
│   │       └── middleware/
│   │           ├── auth.rs
│   │           └── logging.rs
│   └── agent/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── config.rs
│           ├── collector/
│           │   ├── mod.rs
│           │   ├── cpu.rs
│           │   ├── memory.rs
│           │   ├── disk.rs
│           │   ├── network.rs
│           │   ├── load.rs
│           │   ├── process.rs
│           │   ├── temperature.rs
│           │   └── gpu.rs
│           ├── reporter.rs
│           ├── probe/
│           │   ├── icmp.rs
│           │   ├── tcp.rs
│           │   └── http.rs
│           ├── executor.rs
│           └── terminal.rs
└── web/                      # React SPA
    ├── package.json
    ├── vite.config.ts
    ├── index.html
    └── src/
        ├── main.tsx
        ├── router.tsx
        ├── routes/
        ├── components/
        ├── hooks/
        ├── lib/
        └── types/
```

### 核心依赖

**common crate:**

- `serde` / `serde_json` — 序列化
- `chrono` — 时间处理

**server crate:**

- `axum` — Web 框架
- `tokio` — 异步运行时
- `sea-orm` (feature: `sqlx-sqlite`) — ORM
- `tower-http` — CORS、日志、静态文件
- `rust-embed` — 嵌入前端静态资源
- `argon2` — 密码哈希
- `uuid` — ID 生成
- `utoipa` + `utoipa-swagger-ui` — OpenAPI 文档
- `tokio-cron-scheduler` — 定时任务
- `tracing` + `tracing-subscriber` — 日志
- `dashmap` — 并发 HashMap (Agent 连接管理)
- `maxminddb` — GeoIP
- `figment` — 配置加载 (TOML + 环境变量)
- `reqwest` — 通知渠道 HTTP 调用
- `lettre` — Email 发送

**agent crate:**

- `sysinfo` — 系统指标采集
- `tokio-tungstenite` — WebSocket 客户端
- `surge-ping` — ICMP Ping
- `portable-pty` — 终端 PTY
- `nvml-wrapper` — NVIDIA GPU (可选 feature)
- `self_update` — 自动更新
- `figment` — 配置
- `tracing` — 日志

**前端:**

- React 19+ / Vite / Tailwind CSS v4 / shadcn/ui
- TanStack Router / TanStack Query
- `openapi-typescript` — 从 OpenAPI spec 生成 TS 类型
- `recharts` — 图表
- `xterm.js` — Web 终端

---

## 2. 数据模型

### 2.1 用户与认证

```rust
// users
id: String (UUID, PK)
username: String (unique)
password_hash: String (argon2)
role: String ("admin" | "member")
totp_secret: Option<String>           // P1: 2FA
created_at: DateTimeUtc
updated_at: DateTimeUtc

// sessions
id: String (UUID, PK)
user_id: String (FK -> users)
token: String (unique, indexed)
ip: String
user_agent: String
expires_at: DateTimeUtc
created_at: DateTimeUtc

// api_keys
id: String (UUID, PK)
user_id: String (FK -> users)
name: String
key_hash: String (argon2)
key_prefix: String                     // 前 8 位明文，用于识别
last_used_at: Option<DateTimeUtc>
created_at: DateTimeUtc
```

### 2.2 服务器与 Agent

```rust
// servers
id: String (UUID, PK)
token_hash: String                     // Agent Token, argon2 哈希存储
token_prefix: String                   // 前 8 位明文, 用于快速查找
name: String
// 静态信息 (Agent 上报)
cpu_name: Option<String>
cpu_cores: Option<i32>
cpu_arch: Option<String>
os: Option<String>
kernel_version: Option<String>
mem_total: Option<i64>                 // bytes
swap_total: Option<i64>
disk_total: Option<i64>
ipv4: Option<String>
ipv6: Option<String>
region: Option<String>                 // GeoIP
country_code: Option<String>
virtualization: Option<String>
agent_version: Option<String>
// 管理信息
group_id: Option<String> (FK -> server_groups)
weight: i32 (default 0)
hidden: bool (default false)
remark: Option<String>
public_remark: Option<String>
// 计费 (P2)
price: Option<f64>
billing_cycle: Option<String>
currency: Option<String>
expired_at: Option<DateTimeUtc>
// 流量限额
traffic_limit: Option<i64>            // bytes
traffic_limit_type: Option<String>     // "sum" | "up" | "down"
created_at: DateTimeUtc
updated_at: DateTimeUtc

// server_groups
id: String (UUID, PK)
name: String (unique)
weight: i32 (default 0)
created_at: DateTimeUtc

// server_tags (多对多)
server_id: String (FK -> servers)
tag: String
PK: (server_id, tag)
```

### 2.3 指标记录

```rust
// records — 1 分钟聚合
id: i64 (autoincrement, PK)
server_id: String (indexed)
time: DateTimeUtc (indexed)
// 复合索引: (server_id, time)
cpu: f64                               // %
mem_used: i64                          // bytes
swap_used: i64
disk_used: i64
net_in_speed: i64                      // bytes/sec
net_out_speed: i64
net_in_transfer: i64                   // bytes 累计
net_out_transfer: i64
load1: f64
load5: f64
load15: f64
tcp_conn: i32
udp_conn: i32
process_count: i32
temperature: Option<f64>              // C
gpu_usage: Option<f64>                // % 平均

// records_hourly — 小时聚合 (长期保留)
// 同 records 结构，值为该小时的平均值
id: i64 (autoincrement, PK)
server_id: String (indexed)
time: DateTimeUtc (indexed)
// ... 同上字段

// gpu_records — GPU 每卡详情
id: i64 (autoincrement, PK)
server_id: String (indexed)
time: DateTimeUtc (indexed)
device_index: i32
device_name: String
mem_total: i64
mem_used: i64
utilization: f64                       // %
temperature: f64                       // C
```

### 2.4 告警与通知

```rust
// alert_rules
id: String (UUID, PK)
name: String
enabled: bool (default true)
rules_json: String                     // JSON: Vec<AlertRuleItem>
trigger_mode: String                   // "always" | "once"
notification_group_id: Option<String> (FK)
fail_trigger_tasks: Option<String>     // JSON: Vec<String>
recover_trigger_tasks: Option<String>
cover_type: String                     // "all" | "include" | "exclude"
server_ids_json: Option<String>        // JSON: Vec<String>
created_at: DateTimeUtc
updated_at: DateTimeUtc

// notifications
id: String (UUID, PK)
name: String
notify_type: String                    // "webhook" | "telegram" | "email" | "bark"
config_json: String                    // JSON: 各渠道配置
enabled: bool (default true)
created_at: DateTimeUtc

// notification_groups
id: String (UUID, PK)
name: String
notification_ids_json: String          // JSON: Vec<String>
created_at: DateTimeUtc
```

### 2.5 探测与任务

```rust
// ping_tasks
id: String (UUID, PK)
name: String
probe_type: String                     // "icmp" | "tcp" | "http"
target: String
interval: i32                          // 秒
server_ids_json: String                // JSON: 执行的 Agent 列表
enabled: bool (default true)
created_at: DateTimeUtc

// ping_records
id: i64 (autoincrement, PK)
task_id: String (indexed)
server_id: String (indexed)
latency: f64                           // ms
success: bool
error: Option<String>
time: DateTimeUtc (indexed)
// 复合索引: (task_id, server_id, time)

// tasks — 远程命令
id: String (UUID, PK)
command: String
server_ids_json: String
created_by: String (FK -> users)
created_at: DateTimeUtc

// task_results
id: i64 (autoincrement, PK)
task_id: String (FK -> tasks)
server_id: String
output: String
exit_code: i32
finished_at: DateTimeUtc
```

### 2.6 系统配置与审计

```rust
// configs (K-V)
key: String (PK)
value: String                          // JSON

// audit_logs (P2)
id: i64 (autoincrement, PK)
user_id: String
action: String
detail: Option<String>                 // JSON
ip: String
created_at: DateTimeUtc
```

### 2.7 数据保留与聚合策略

```
Agent 上报 (每 3 秒)
  -> 内存缓存: 仅保留每台 Server 最新一份，用于实时推送
  -> 每 1 分钟: 写入 records 表
  -> 每 1 小时: 聚合写入 records_hourly (平均值)
  -> 清理策略:
      records:        保留 7 天
      records_hourly: 保留 90 天
      gpu_records:    保留 7 天
      ping_records:   保留 7 天
      audit_logs:     保留 180 天
```

清理由 `tokio-cron-scheduler` 每小时执行一次。

---

## 3. 通信协议

### 3.1 Agent <-> Server WebSocket

**连接地址:** `ws://<server>/api/agent/ws?token=<agent_token>`

**握手流程:**

```
Agent                              Server
  |                                  |
  |--- WebSocket 连接 + token ------>|
  |                                  | 验证 token -> 查找 server
  |<-- Welcome { server_id,         |
  |     protocol_version: 1,        |
  |     report_interval: 3 } -------|
  |                                  |
  |--- SystemInfo { cpu_name,       |  首次/重启后上报静态信息
  |     os, mem_total, ... } ------>|
  |<-- Ack { msg_id } -------------|
  |                                  |
  |--- Report { cpu, mem, ... } --->|  每 3 秒
  |--- Report --------------------->|
  |    ...                           |
  |                                  |
  |<-- PingTasksSync { tasks } ----|  下发探测任务
  |--- PingResult { task_id,       |
  |     latency, success } -------->|
  |                                  |
  |<-- Exec { task_id, command } ---|  下发命令
  |--- TaskResult { task_id,       |
  |     output, exit_code } ------->|
  |                                  |
  |<-- Ping ------------------------|  心跳 (30 秒)
  |--- Pong ----------------------->|
```

**消息定义 (common crate):**

```rust
// Agent -> Server
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentMessage {
    SystemInfo(SystemInfo),
    Report(SystemReport),
    PingResult(PingResult),
    TaskResult(TaskResult),
    Pong,
}

// Server -> Agent
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome { server_id: String, protocol_version: u32, report_interval: u32 },
    Ack { msg_id: String },
    PingTasksSync { tasks: Vec<PingTaskConfig> },
    Exec { task_id: String, command: String, timeout: Option<u32> },  // timeout 秒, 默认 300
    TerminalClose { session_id: String },
    Ping,
    // P2
    Upgrade { version: String, download_url: String },
}

// 终端数据使用 WebSocket Binary 帧传输，不经过 JSON 封装。
// Binary 帧格式: [1 byte session_id 长度][session_id bytes][payload bytes]
// 这适用于 Agent -> Server 的 TerminalOutput 和 Server -> Agent 的 TerminalInput。
```

**ACK 机制:**

- `SystemInfo` -> 需要 `Ack` (确保静态信息已持久化)
- `Report` -> 不需要 ACK (高频，丢失可接受)
- `Exec` / `TaskResult` -> 需要 ACK (命令执行不可丢)
- `PingResult` -> 不需要 ACK

需要 ACK 的消息携带 `msg_id` 字段 (UUID)，Server 回复 `Ack { msg_id }` 进行关联。Agent 端在 5 秒内未收到 ACK 可选择重发。

**消息大小限制:**

- WebSocket JSON 帧最大 1MB
- `TaskResult.output` 最大 512KB，超出部分截断
- Binary 帧 (终端数据) 最大 64KB

**命令执行安全:**

- 命令执行超时: 默认 300 秒 (5 分钟)，可在 `Exec` 消息中指定 `timeout`
- 输出大小限制: 最大 512KB
- 并发执行限制: 每台 Agent 最多 5 个并发命令
- 命令长度限制: 最大 8KB

**重连策略:**

- 指数退避: 1s -> 2s -> 4s -> 8s -> 16s -> 30s (上限)
- 添加 +/-20% 随机抖动，避免雷群效应
- 重连后自动重新上报 `SystemInfo`

### 3.2 Server -> Browser WebSocket

**连接地址:** `ws://<server>/api/ws/servers`

需要携带 Session Cookie 或 `?api_key=<key>` 认证。

**推送消息:**

```rust
// Server -> Browser
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserMessage {
    FullSync { servers: Vec<ServerStatus> },
    Update { servers: Vec<ServerStatus> },
    ServerOnline { server_id: String },
    ServerOffline { server_id: String },
}

pub struct ServerStatus {
    pub id: String,
    pub name: String,
    pub online: bool,
    pub last_active: i64,
    pub uptime: u64,
    pub cpu: f64,
    pub mem_used: i64,
    pub mem_total: i64,
    pub swap_used: i64,
    pub swap_total: i64,
    pub disk_used: i64,
    pub disk_total: i64,
    pub net_in_speed: i64,
    pub net_out_speed: i64,
    pub net_in_transfer: i64,
    pub net_out_transfer: i64,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub tcp_conn: i32,
    pub udp_conn: i32,
    pub process_count: i32,
    pub cpu_name: Option<String>,
    pub os: Option<String>,
    pub region: Option<String>,
    pub country_code: Option<String>,
}
```

**推送机制:**

- 使用 `tokio::sync::broadcast` 通道
- Agent 上报 -> AgentManager 更新缓存 -> 广播到所有 Browser 订阅者
- 推送频率: 最快每 3 秒一次 (跟随 Agent 上报间隔)
- Browser 只接收不发送 (单向推送，终端除外)

### 3.3 Web 终端

**连接地址:** `ws://<server>/api/ws/terminal/<server_id>`

```
Browser <-WebSocket-> Server <-WebSocket-> Agent
                      (代理转发)
```

Server 作为中间代理，将 Browser 的输入转发给 Agent 的 PTY，将 Agent 的 PTY 输出转发给 Browser。用 `session_id` 关联两端。终端数据使用 WebSocket Binary 帧直接转发，不经过 JSON 封装。

**终端安全:**

- **PTY 运行用户**: Agent 以哪个系统用户启动，PTY 就以该用户身份运行。安装脚本默认以 root 运行 Agent (systemd service)，文档/安全提示中建议用户按需降权
- **会话限制**: 每台 Server 最多 3 个并发终端会话
- **空闲超时**: 无输入 10 分钟后自动断开
- **消息大小**: 单条 WebSocket 帧最大 64KB
- **审计**: 终端连接/断开事件记录到 audit_logs (P2)

---

## 4. 服务端内部架构

### 4.1 AppState

```rust
pub struct AppState {
    pub db: DatabaseConnection,
    pub agent_manager: AgentManager,
    pub browser_tx: broadcast::Sender<BrowserMessage>,
    pub config: AppConfig,
}
```

通过 `Arc<AppState>` 在所有 handler 间共享。

### 4.2 AgentManager

```rust
pub struct AgentManager {
    connections: DashMap<String, AgentConnection>,
    latest_reports: DashMap<String, CachedReport>,
    browser_tx: broadcast::Sender<BrowserMessage>,
}

pub struct AgentConnection {
    pub server_id: String,
    pub tx: mpsc::Sender<ServerMessage>,
    pub connected_at: Instant,
    pub last_report_at: Instant,
    pub remote_addr: SocketAddr,
}

pub struct CachedReport {
    pub report: SystemReport,
    pub received_at: Instant,
}
```

**职责:**

1. Agent 连接/断开时维护 `connections` 表
2. 收到 Report 时更新 `latest_reports` 并广播到 `browser_tx`
3. 提供查询接口: 在线列表、单台状态、连接数
4. 转发命令/终端数据到指定 Agent

**离线检测:** 后台 tokio 任务每 10 秒扫描 `connections`，`last_report_at` 超过 30 秒判定为离线，移除连接并广播 `ServerOffline` 事件。

### 4.3 Service 层

Service 方法为关联函数，接受 `&DatabaseConnection` 参数 (sea-orm 的 `DatabaseConnection` 实现了 `Clone`)，避免生命周期约束:

```rust
pub struct RecordService;

impl RecordService {
    pub async fn save_report(db: &DatabaseConnection, server_id: &str, report: &SystemReport) -> Result<()>;
    pub async fn query_history(db: &DatabaseConnection, server_id: &str, from: DateTime, to: DateTime) -> Result<Vec<record::Model>>;
    pub async fn aggregate_hourly(db: &DatabaseConnection) -> Result<u64>;
    pub async fn cleanup_expired(db: &DatabaseConnection, retention_days: u32) -> Result<u64>;
}
```

**Service 清单:**

| Service | 职责 |
|---------|------|
| `AuthService` | 用户登录验证 (argon2)、Session 创建/销毁、API Key 校验 |
| `ServerService` | 服务器 CRUD、分组、标签、排序 |
| `RecordService` | 指标写入、历史查询、小时聚合、过期清理 |
| `AlertService` | 告警规则 CRUD、规则评估 (每分钟)、触发通知 |
| `NotificationService` | 通知渠道管理、消息分发 (Webhook/Telegram/Email/Bark) |
| `PingService` | Ping 任务 CRUD、任务分发到 Agent、结果存储 |
| `TaskService` | 远程命令下发、结果查询 |
| `ConfigService` | 系统配置读写 (K-V) |
| `GeoIpService` | IP -> 地理位置查询 (MaxMind MMDB) |
| `UserService` | 用户 CRUD、角色管理 (P2 多用户) |

### 4.4 后台任务

| 任务 | 频率 | 职责 |
|------|------|------|
| 指标写入 | 每 1 分钟 | 从 `latest_reports` 取最新值写入 `records` 表 |
| 告警评估 | 每 1 分钟 (指标写入后) | 遍历启用的告警规则，评估各 Server 指标 |
| 离线检测 | 每 10 秒 | 扫描 Agent 连接状态，触发离线事件和告警 |
| 小时聚合 | 每 1 小时 | `records` -> `records_hourly` 聚合平均值 |
| 数据清理 | 每 1 小时 (聚合后) | 删除过期的 records / ping_records / gpu_records |
| Ping 任务同步 | 配置变更时 | 向相关 Agent 推送 `PingTasksSync` |
| Session 清理 | 每 1 小时 | 删除过期的 Session |

**执行顺序保证:** 每小时任务按顺序执行: 小时聚合 -> 数据清理，确保不会清理掉尚未聚合的数据。每分钟任务: 指标写入 -> 告警评估，确保告警评估读到最新写入的记录。

### 4.5 请求处理流程

**Agent 指标上报:**

```
Agent WebSocket 消息
  -> ws::agent handler 解析 AgentMessage::Report
  -> agent_manager.update_report(server_id, report)
    -> 更新 latest_reports 缓存
    -> 通过 browser_tx 广播 BrowserMessage::Update
  -> (后台任务每分钟批量写 DB)
```

**浏览器查看仪表盘:**

```
GET /api/servers
  -> auth middleware 校验 Session Cookie
  -> api::server handler
  -> ServerService.list_with_status()
    -> 查询 servers 表
    -> 合并 agent_manager.latest_reports 实时数据
  -> 返回 JSON

WebSocket /api/ws/servers
  -> auth middleware 校验
  -> 发送 FullSync (当前所有 Server 状态)
  -> 订阅 browser_tx，持续推送 Update
```

**告警评估:**

```
后台任务每 1 分钟触发
  -> AlertService.evaluate_all()
    -> 加载所有 enabled 的 alert_rules
    -> 遍历每条规则的 server 范围
    -> 从 latest_reports 取实时值 + 查询最近 N 分钟 records
    -> 判定是否超阈值 (70%+ 采样超阈值才触发)
    -> 触发模式判断 (always / once)
    -> 调用 NotificationService.send() 分发通知
```

### 4.6 启动流程

```rust
#[tokio::main]
async fn main() {
    // 1. 初始化 tracing 日志
    // 2. 加载配置 (TOML + 环境变量)
    // 3. 连接数据库 + 运行迁移
    // 4. 构建 AppState
    // 5. 初始化 admin 用户 (如 users 表为空)
    // 6. 生成 auto_discovery_key (如未配置)
    // 7. 启动后台任务 (tokio::spawn)
    // 8. 构建 Axum Router
    // 9. 绑定端口，启动 HTTP 服务
    // 10. 等待 SIGTERM/SIGINT -> 优雅关闭
}
```

---

## 5. 认证与授权

### 5.1 三条认证路径

```
请求进入
  |
  +-- Cookie: session_token=xxx  -> Session 认证 (浏览器)
  +-- Header: X-API-Key: sb_xxx  -> API Key 认证 (自动化)
  +-- Query: ?token=xxx          -> Agent Token 认证 (Agent WebSocket)
```

### 5.2 Session 认证

**登录流程:**

```
POST /api/auth/login
Body: { username, password }
  -> argon2 验证密码
  -> 生成 session_token (随机 32 字节 -> base64url)
  -> 写入 sessions 表
  -> Set-Cookie: session_token=xxx; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400
  -> 返回 { user_id, username, role }
```

**请求校验:**

```
Cookie: session_token=xxx
  -> 查询 sessions 表
  -> 检查 expires_at 未过期
  -> 注入 CurrentUser { user_id, role } 到 Axum Extension
```

**Session 续期:** 每次有效请求自动延长 `expires_at` (滑动过期，默认 24 小时)。

### 5.3 API Key 认证

**Key 格式:** `sb_` + 32 字节随机 base64url

**创建:** 明文只返回一次，存储 argon2 哈希 + 前 8 位前缀。

**校验:**

```
Header: X-API-Key: sb_xR4k9m2p...
  -> 用 key_prefix 缩小查询范围
  -> argon2 verify 匹配
  -> 更新 last_used_at
  -> 注入 CurrentUser
```

### 5.4 Agent Token 认证

**注册流程:**

```
POST /api/agent/register
Header: Authorization: Bearer <auto_discovery_key>
  -> 验证 auto_discovery_key
  -> 创建 server 记录，生成 token
  -> 返回 { server_id, token }
```

**WebSocket 连接:** Query param `?token=xxx` 直接在 handler 内验证。

### 5.5 角色权限矩阵

| 资源 | Admin | Member | API Key | Agent | 未认证 |
|------|-------|--------|---------|-------|--------|
| 服务器列表/详情 | Y | Y (own) | Y | - | P2 公开页 |
| 服务器增删改 | Y | N | Y | - | N |
| 告警规则管理 | Y | N | Y | - | N |
| 通知配置 | Y | N | N | - | N |
| 用户管理 | Y | N | N | - | N |
| 系统设置 | Y | N | N | - | N |
| 指标上报 | - | - | - | Y | N |
| 实时 WebSocket | Y | Y | Y | - | N |
| Web 终端 | Y | N | N | - | N |
| 远程命令 | Y | N | Y | - | N |

P0 阶段只实现 Admin 角色，Member 在 P2 引入。

### 5.6 Middleware

```rust
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Response {
    let current_user = if let Some(user) = try_session(&state, &req).await {
        Some(user)
    } else {
        try_api_key(&state, &req).await
    };

    match current_user {
        Some(user) => {
            req.extensions_mut().insert(user);
            next.run(req).await
        }
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}
```

### 5.7 初始化

首次启动 `users` 表为空时:

- 从配置/环境变量读取 `ADMIN_USERNAME` + `ADMIN_PASSWORD`
- 如果未配置，生成随机密码并打印到日志
- 创建 admin 用户

---

## 6. 告警系统

### 6.1 告警规则类型

**资源阈值类:**

| 类型 | 判定 |
|------|------|
| `cpu` / `memory` / `swap` / `disk` | 使用率 % |
| `load1` / `load5` / `load15` | 系统负载 |
| `temperature` | 温度 C |
| `gpu` | GPU 平均使用率 % |
| `tcp_conn` / `udp_conn` | 连接数 |
| `process` | 进程数 |
| `net_in_speed` / `net_out_speed` | 网络速率 bytes/sec |

**流量周期类:**

| 类型 | 说明 |
|------|------|
| `transfer_in_cycle` | 周期入站累计流量 |
| `transfer_out_cycle` | 周期出站累计流量 |
| `transfer_all_cycle` | 周期双向累计流量 |

周期选项: `hour` / `day` / `week` / `month` / `year`

**离线检测:** `offline` — 持续离线超过 `duration` 秒后触发。

### 6.2 规则结构

```rust
pub struct AlertRuleItem {
    pub rule_type: AlertRuleType,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub duration: Option<u32>,              // 秒，offline 用
    pub cycle_interval: Option<String>,     // 流量周期
    pub cycle_limit: Option<i64>,           // 流量限额 (bytes)
}
```

一条 `AlertRule` 包含多个 `AlertRuleItem`，**所有 Item 同时满足 (AND) 才触发**。

### 6.3 评估流程

```
每 1 分钟执行:

for rule in enabled_alert_rules:
    servers = resolve_servers(rule.cover_type, rule.server_ids)

    for server in servers:
        all_items_triggered = true

        for item in rule.rules:
            match item.rule_type:
                资源阈值类:
                    -> 查询最近 10 分钟 records (10 个采样点)
                    -> 70%+ 采样超阈值 -> 触发
                    -> 否则 -> all_items_triggered = false; break

                流量周期类:
                    -> 计算当前周期起止时间
                    -> 取周期起始和最新的 transfer 差值
                    -> 超过 cycle_limit -> 触发

                offline:
                    -> 检查 agent_manager 中 last_report_at
                    -> 离线时长 > duration -> 触发

        if all_items_triggered:
            match rule.trigger_mode:
                "always" -> 每次都通知 (最短间隔 5 分钟防抖)
                "once"   -> 未通知过或已恢复后再次触发 -> 通知

            -> NotificationService.send()
            -> 执行 fail_trigger_tasks (如有)

        else if was_previously_triggered:
            -> 标记恢复
            -> 执行 recover_trigger_tasks (如有)
```

### 6.4 告警状态管理

告警触发状态持久化到 SQLite，避免服务端重启后 `once` 模式规则重复告警。

```rust
// alert_states 表
id: i64 (autoincrement, PK)
rule_id: String (indexed)
server_id: String (indexed)
// 复合唯一索引: (rule_id, server_id)
first_triggered_at: DateTimeUtc
last_notified_at: DateTimeUtc
count: i32
resolved: bool (default false)
resolved_at: Option<DateTimeUtc>
updated_at: DateTimeUtc
```

内存中维护热缓存加速评估，启动时从 `alert_states` 表加载未恢复的记录：

```rust
pub struct AlertState {
    // 热缓存，启动时从 DB 加载
    triggered: DashMap<(String, String), TriggeredInfo>,
    db: DatabaseConnection,
}

pub struct TriggeredInfo {
    pub first_triggered_at: DateTime<Utc>,
    pub last_notified_at: DateTime<Utc>,
    pub count: u32,
}

impl AlertState {
    /// 启动时从 alert_states 加载未恢复的记录
    pub async fn load_from_db(db: &DatabaseConnection) -> Result<Self>;
    /// 触发时同步写入 DB + 更新缓存
    pub async fn mark_triggered(&self, rule_id: &str, server_id: &str) -> Result<()>;
    /// 恢复时同步写入 DB + 更新缓存
    pub async fn mark_resolved(&self, rule_id: &str, server_id: &str) -> Result<()>;
}
```

### 6.5 通知渠道

| 渠道 | 配置 | 实现 |
|------|------|------|
| Webhook | url, method, headers, body_template | reqwest |
| Telegram | bot_token, chat_id | Telegram Bot API |
| Email | smtp_host, port, username, password, from, to | lettre SMTP |
| Bark | server_url, device_key | HTTP GET |

**模板变量:**

`{{server_name}}`, `{{server_id}}`, `{{rule_name}}`, `{{event}}`, `{{message}}`, `{{time}}`, `{{cpu}}`, `{{memory}}`, ...

**默认模板:**

```
[ServerBee] {{server_name}} {{event}}
{{message}}
时间: {{time}}
```

**通知防抖:**

- `always` 模式同一 (rule, server) 最短间隔 5 分钟
- 发送失败不重试，记录日志
- 通知队列使用 `tokio::sync::mpsc`，异步发送

---

## 7. API 设计

### 7.1 路由总览

认证标注: `S` = Session, `K` = API Key, `A` = Agent Token

**认证:**

```
POST   /api/auth/login                    [公开]
POST   /api/auth/logout                   [S]
GET    /api/auth/me                       [S|K]
POST   /api/auth/api-keys                 [S]
GET    /api/auth/api-keys                 [S]
DELETE /api/auth/api-keys/:id             [S]
PUT    /api/auth/password                 [S]
```

**服务器管理:**

```
GET    /api/servers                       [S|K]
GET    /api/servers/:id                   [S|K]
PUT    /api/servers/:id                   [S|K]
DELETE /api/servers/:id                   [S|K]
GET    /api/servers/:id/records           [S|K]   ?from=&to=&interval=
GET    /api/servers/:id/gpu-records       [S|K]
POST   /api/servers/batch-delete          [S|K]
```

**服务器分组:**

```
GET    /api/server-groups                 [S|K]
POST   /api/server-groups                 [S|K]
PUT    /api/server-groups/:id             [S|K]
DELETE /api/server-groups/:id             [S|K]
```

**告警规则:**

```
GET    /api/alert-rules                   [S|K]
POST   /api/alert-rules                   [S|K]
PUT    /api/alert-rules/:id               [S|K]
DELETE /api/alert-rules/:id               [S|K]
```

**通知:**

```
GET    /api/notifications                 [S]
POST   /api/notifications                 [S]
PUT    /api/notifications/:id             [S]
DELETE /api/notifications/:id             [S]
POST   /api/notifications/:id/test        [S]
GET    /api/notification-groups           [S]
POST   /api/notification-groups           [S]
PUT    /api/notification-groups/:id       [S]
DELETE /api/notification-groups/:id       [S]
```

**Ping 探测:**

```
GET    /api/ping-tasks                    [S|K]
POST   /api/ping-tasks                    [S|K]
PUT    /api/ping-tasks/:id                [S|K]
DELETE /api/ping-tasks/:id                [S|K]
GET    /api/ping-tasks/:id/records        [S|K]   ?from=&to=&server_id=
```

**远程命令:**

```
POST   /api/tasks                         [S|K]
GET    /api/tasks/:id                     [S|K]
GET    /api/tasks/:id/results             [S|K]
```

**系统设置:**

```
GET    /api/settings                      [S]
PUT    /api/settings                      [S]
GET    /api/settings/auto-discovery-key   [S]
PUT    /api/settings/auto-discovery-key   [S]
```

**Agent 专用:**

```
POST   /api/agent/register                [Bearer]
GET    /api/agent/ws                      [A, query]
```

**WebSocket:**

```
GET    /api/ws/servers                    [S|K]
GET    /api/ws/terminal/:server_id        [S]
```

**P2 扩展:**

```
GET    /api/users                         [S:admin]
POST   /api/users                         [S:admin]
PUT    /api/users/:id                     [S:admin]
DELETE /api/users/:id                     [S:admin]
GET    /api/audit-logs                    [S:admin]
GET    /api/status                        [公开]
```

### 7.2 统一响应格式

```json
// 成功
{ "data": T }

// 分页
{ "data": [T], "total": 100, "page": 1, "page_size": 20 }

// 错误
{ "error": { "code": "UNAUTHORIZED", "message": "Invalid session token" } }
```

**错误码:**

| HTTP | code | 说明 |
|------|------|------|
| 400 | `BAD_REQUEST` | 请求参数错误 |
| 401 | `UNAUTHORIZED` | 未认证 |
| 403 | `FORBIDDEN` | 权限不足 |
| 404 | `NOT_FOUND` | 资源不存在 |
| 409 | `CONFLICT` | 资源冲突 |
| 422 | `VALIDATION_ERROR` | 数据校验失败 |
| 500 | `INTERNAL_ERROR` | 内部错误 |

### 7.3 OpenAPI

使用 `utoipa` 自动生成 OpenAPI 3.0 spec:

- Swagger UI: `GET /swagger-ui/`
- JSON spec: `GET /api/openapi.json`
- 前端: `openapi-typescript` 从 spec 生成 TS 类型

### 7.4 指标查询参数

`GET /api/servers/:id/records`:

| 参数 | 类型 | 说明 |
|------|------|------|
| `from` | ISO 8601 | 起始时间 |
| `to` | ISO 8601 | 结束时间 |
| `interval` | string | `raw` / `hourly` / `auto` |

`auto` 策略: <= 24 小时用 `raw`，> 24 小时用 `hourly`。

---

## 8. Agent 端架构

### 8.1 内部结构

```
Agent
  +-- Config (TOML)
  +-- Collector (采集层)
  |     +-- CpuCollector
  |     +-- MemoryCollector
  |     +-- DiskCollector
  |     +-- NetworkCollector
  |     +-- LoadCollector
  |     +-- ProcessCollector
  |     +-- TempCollector
  |     +-- GpuCollector
  +-- Reporter (WebSocket 上报+重连)
  +-- ProbeManager (探测层)
  |     +-- IcmpProbe
  |     +-- TcpProbe
  |     +-- HttpProbe
  +-- Executor (命令执行)
  +-- Terminal (PTY 会话)
```

### 8.2 采集层

```rust
pub struct Collector {
    sys: sysinfo::System,
    networks: sysinfo::Networks,
    prev_net_in: u64,
    prev_net_out: u64,
    prev_time: Instant,
}

impl Collector {
    pub fn collect(&mut self) -> SystemReport;
    pub fn system_info(&self) -> SystemInfo;
}
```

**采集频率:** 默认每 3 秒，可被 Server `Welcome.report_interval` 覆盖。

**网络速率:** `speed = (current_transfer - prev_transfer) / elapsed_seconds`

**采集来源:**

| 指标 | 来源 |
|------|------|
| CPU 使用率/型号/核数/架构 | `sysinfo::System` |
| 内存/Swap | `sysinfo::System` |
| 磁盘 | `sysinfo::Disks` |
| 网络速率/累计 | `sysinfo::Networks` + 差值计算 |
| 负载 | `sysinfo::System::load_average()` |
| 进程数 | `sysinfo::System::processes().len()` |
| TCP/UDP 连接数 | `/proc/net/tcp` `/proc/net/udp` (Linux), `netstat` (其他) |
| 温度 | `sysinfo::Components` |
| GPU | `nvml-wrapper` (NVIDIA, 可选 feature) |
| OS/内核 | `sysinfo::System` |
| 运行时间 | `sysinfo::System::uptime()` |
| 虚拟化 | `/sys/class/dmi/id/product_name` 或 `systemd-detect-virt` |

### 8.3 Reporter

```rust
pub struct Reporter {
    server_url: String,
    token: String,
    collector: Collector,
    probe_manager: ProbeManager,
    terminal_sessions: HashMap<String, PtySession>,
}

impl Reporter {
    pub async fn run(&mut self) {
        loop {
            match self.connect_and_run().await {
                Ok(()) => {}
                Err(e) => {
                    tracing::warn!("连接断开: {e}");
                    self.backoff_sleep().await;
                }
            }
        }
    }

    async fn connect_and_run(&mut self) -> Result<()> {
        let ws = connect(&self.server_url, &self.token).await?;
        let (mut write, mut read) = ws.split();

        let welcome = read.next().await?;
        let interval = welcome.report_interval;

        write.send(AgentMessage::SystemInfo(self.collector.system_info())).await?;

        let mut report_ticker = tokio::time::interval(Duration::from_secs(interval));

        loop {
            tokio::select! {
                _ = report_ticker.tick() => {
                    let report = self.collector.collect();
                    write.send(AgentMessage::Report(report)).await?;
                }
                msg = read.next() => {
                    match msg? {
                        ServerMessage::Ping => write.send(AgentMessage::Pong).await?,
                        ServerMessage::Exec { task_id, command } => {
                            self.handle_exec(task_id, command, &mut write).await;
                        }
                        ServerMessage::PingTasksSync { tasks } => {
                            self.probe_manager.update_tasks(tasks);
                        }
                        ServerMessage::TerminalInput { session_id, data } => {
                            self.handle_terminal_input(session_id, data).await;
                        }
                        ServerMessage::TerminalClose { session_id } => {
                            self.terminal_sessions.remove(&session_id);
                        }
                        _ => {}
                    }
                }
                result = self.probe_manager.next_result() => {
                    write.send(AgentMessage::PingResult(result)).await?;
                }
            }
        }
    }
}
```

### 8.4 探测层

```rust
pub struct ProbeManager {
    tasks: Vec<ProbeTask>,
    result_tx: mpsc::Sender<PingResult>,
    result_rx: mpsc::Receiver<PingResult>,
    handles: Vec<JoinHandle<()>>,
}

impl ProbeManager {
    pub fn update_tasks(&mut self, tasks: Vec<PingTaskConfig>);
    pub async fn next_result(&mut self) -> PingResult;
}
```

| 类型 | 实现 | 超时 |
|------|------|------|
| ICMP | `surge-ping`，需 `CAP_NET_RAW` | 5 秒 |
| TCP | `tokio::net::TcpStream::connect()` | 5 秒 |
| HTTP | `reqwest::Client::get(url)` | 10 秒 |

### 8.5 配置

```toml
# /etc/serverbee/agent.toml

server_url = "ws://your-server:9527"
token = "agent-uuid-token"

[collector]
interval = 3
enable_gpu = false
enable_temperature = true

[log]
level = "info"
file = "/var/log/serverbee-agent.log"
```

环境变量覆盖: `SB_SERVER_URL`, `SB_TOKEN`, `SB_LOG_LEVEL`

### 8.6 注册流程

```
首次运行 (无 token):
  1. 读取 server_url + auto_discovery_key
  2. POST /api/agent/register (Bearer: auto_discovery_key)
  3. 获取 { server_id, token }
  4. 写回配置文件
  5. 开始正常上报

后续运行 (有 token):
  1. 直接连接 WebSocket
  2. 发送 SystemInfo
  3. 开始上报
```

### 8.7 平台支持

| 平台 | 级别 | 说明 |
|------|------|------|
| Linux (amd64/arm64) | 完整 | 主要目标 |
| macOS (amd64/arm64) | 完整 | 开发/测试 |
| Windows (amd64) | 基本 | TCP/UDP 连接数采集方式不同 |
| FreeBSD | 基本 | `sysinfo` 支持有限 |

### 8.8 自更新 (P2)

```
Server 下发 ServerMessage::Upgrade { version, download_url }
  -> 下载新版本到临时目录
  -> 校验 checksum
  -> 替换自身二进制
  -> 重启进程
```

---

## 9. 前端架构

### 9.1 目录结构

```
web/
├── package.json
├── vite.config.ts
├── app.css                   # TW v4 CSS-first 配置
├── index.html
├── src/
│   ├── main.tsx
│   ├── router.tsx
│   ├── routes/
│   │   ├── __root.tsx            # Root layout (侧边栏 + 顶栏)
│   │   ├── login.tsx             # 登录页 (公开)
│   │   ├── _authed.tsx           # 认证 layout guard
│   │   ├── _authed/
│   │   │   ├── index.tsx              # 仪表盘总览
│   │   │   ├── servers/
│   │   │   │   ├── index.tsx          # 服务器列表
│   │   │   │   └── $id.tsx            # 服务器详情 + 图表
│   │   │   ├── alerts.tsx             # 告警规则管理
│   │   │   ├── notifications.tsx      # 通知渠道管理
│   │   │   ├── ping-tasks.tsx         # 探测任务管理
│   │   │   ├── terminal/
│   │   │   │   └── $serverId.tsx      # Web 终端
│   │   │   └── settings/
│   │   │       ├── index.tsx          # 常规设置
│   │   │       ├── api-keys.tsx       # API Key 管理
│   │   │       └── users.tsx          # 用户管理 (P2)
│   ├── components/
│   │   ├── ui/                   # shadcn/ui
│   │   ├── layout/
│   │   │   ├── sidebar.tsx
│   │   │   ├── header.tsx
│   │   │   └── theme-toggle.tsx
│   │   ├── server/
│   │   │   ├── server-card.tsx
│   │   │   ├── server-table.tsx
│   │   │   ├── server-detail.tsx
│   │   │   ├── metrics-chart.tsx
│   │   │   └── status-badge.tsx
│   │   ├── alert/
│   │   │   ├── alert-rule-form.tsx
│   │   │   └── alert-rule-list.tsx
│   │   ├── notification/
│   │   │   ├── notification-form.tsx
│   │   │   └── notification-list.tsx
│   │   └── terminal/
│   │       └── terminal-view.tsx
│   ├── hooks/
│   │   ├── use-auth.ts
│   │   ├── use-servers-ws.ts
│   │   ├── use-api.ts
│   │   └── use-terminal-ws.ts
│   ├── lib/
│   │   ├── api-client.ts
│   │   ├── ws-client.ts
│   │   └── utils.ts
│   └── types/
│       └── api.ts                # openapi-typescript 生成
└── public/
    └── favicon.svg
```

### 9.2 认证守卫

```tsx
// routes/_authed.tsx
function AuthedLayout() {
  const auth = useAuth()
  if (auth.isLoading) return <LoadingSkeleton />
  if (!auth.user) throw redirect({ to: '/login' })
  return <Outlet />
}
```

### 9.3 实时数据流

```tsx
// hooks/use-servers-ws.ts
function useServersWebSocket() {
  const queryClient = useQueryClient()

  useEffect(() => {
    const ws = new WebSocketClient('/api/ws/servers')

    ws.onMessage((msg) => {
      switch (msg.type) {
        case 'full_sync':
          queryClient.setQueryData(['servers'], msg.servers)
          break
        case 'update':
          queryClient.setQueryData(['servers'], (old) =>
            mergeServerUpdates(old, msg.servers)
          )
          break
        case 'server_online':
        case 'server_offline':
          queryClient.setQueryData(['servers'], (old) =>
            updateServerStatus(old, msg)
          )
          break
      }
    })

    return () => ws.close()
  }, [])
}
```

WebSocket 消息直接写入 TanStack Query cache，REST API 用于初始加载，WebSocket 用于持续更新，共享 cache key `['servers']`。

### 9.4 页面设计

**仪表盘总览:**

- 顶部统计卡片: 在线/离线/总数、CPU 平均、内存平均、总带宽
- 服务器卡片网格 (按分组): 名称、地区国旗、OS 图标、CPU/内存/磁盘进度条、网络速率、运行时间
- 实时刷新 (WebSocket 驱动)

**服务器详情:**

- 顶部: 基本信息 (OS, CPU, 内存总量, IP, 地区, Agent 版本)
- 指标图表区: CPU, 内存, 磁盘, 网络, 负载各一张折线图
- 时间范围选择器: 1h / 6h / 24h / 7d / 30d
- GPU 面板 (如有)
- 网络累计流量统计

**图表:** Recharts，按时间范围自动选粒度 (raw / hourly)。

### 9.5 WebSocket 客户端

```typescript
class WebSocketClient {
  private url: string
  private ws: WebSocket | null = null
  private reconnectDelay = 1000
  private maxDelay = 30000

  connect() {
    this.ws = new WebSocket(this.url)
    this.ws.onclose = () => this.scheduleReconnect()
    this.ws.onerror = () => this.ws?.close()
    this.ws.onopen = () => { this.reconnectDelay = 1000 }
  }

  private scheduleReconnect() {
    const jitter = this.reconnectDelay * (0.8 + Math.random() * 0.4)
    setTimeout(() => this.connect(), jitter)
    this.reconnectDelay = Math.min(this.reconnectDelay * 2, this.maxDelay)
  }
}
```

### 9.6 构建与嵌入

```
开发模式:
  web/ 下 `bun run dev` -> Vite dev server (HMR)
  Server 端反代 /api -> Rust 后端

生产构建:
  1. `cd web && bun run build` -> web/dist/
  2. Rust 编译时 rust-embed 嵌入 web/dist/
  3. Axum static_files handler 提供静态资源
  4. 非 /api 路径 fallback 到 index.html (SPA 路由)
```

---

## 10. 部署、配置与运维

### 10.1 服务端配置

```toml
# /etc/serverbee/server.toml

[server]
listen = "0.0.0.0:9527"
data_dir = "/var/lib/serverbee"

[database]
path = "serverbee.db"
max_connections = 10
# WAL 模式自动启用，提高并发读性能
# busy_timeout = 5000ms，写冲突时自动重试

[rate_limit]
login_max = 5                          # 登录接口: 每 IP 每分钟最多 5 次
register_max = 3                       # Agent 注册: 每 IP 每分钟最多 3 次

[auth]
session_ttl = 86400
auto_discovery_key = ""

[admin]
username = "admin"
password = ""

[retention]
records_days = 7
records_hourly_days = 90
gpu_records_days = 7
ping_records_days = 7
audit_logs_days = 180

[geoip]
enabled = false
mmdb_path = ""

[log]
level = "info"
file = ""
```

环境变量覆盖: 前缀 `SB_`，层级用 `_` 连接。

**SQLite 配置说明:**

- WAL 模式: 启动时自动执行 `PRAGMA journal_mode=WAL`
- busy_timeout: 设为 5000ms，多个后台任务并发写入时自动等待锁释放
- 连接池: `max_connections = 10`，但 SQLite 写锁是全局的，多连接主要用于并发读
- 同步模式: `PRAGMA synchronous=NORMAL` (WAL 模式下安全且性能更好)

**TLS/HTTPS:**

ServerBee 本身不处理 TLS，生产环境建议使用反向代理:

- **推荐**: Caddy (自动 HTTPS) 或 nginx 反代到 `127.0.0.1:9527`
- Agent 连接: `wss://your-domain/api/agent/ws?token=xxx`
- 确保 Cookie 的 `Secure` 属性在 HTTPS 环境下启用

**速率限制:**

基于 `tower` middleware 实现，内存中维护 IP -> 计数器映射:

- `POST /api/auth/login`: 每 IP 每分钟最多 5 次
- `POST /api/agent/register`: 每 IP 每分钟最多 3 次
- 超限返回 `429 Too Many Requests`

### 10.2 启动流程

```
./serverbee-server
  1. 解析配置 (TOML + 环境变量)
  2. 初始化 tracing 日志
  3. 确保 data_dir 存在
  4. 连接 SQLite + 运行 sea-orm 迁移
  5. 初始化 admin 用户 (如 users 表为空)
  6. 生成 auto_discovery_key (如未配置)
  7. 构建 AppState
  8. 启动后台任务
  9. 构建 Axum Router
  10. 绑定端口，启动 HTTP 服务
  11. 等待 SIGTERM/SIGINT -> 优雅关闭
```

### 10.3 Docker

```dockerfile
FROM oven/bun:latest AS web-builder
WORKDIR /app/web
COPY web/ .
RUN bun install && bun run build

FROM rust:1.85-alpine AS rust-builder
WORKDIR /app
COPY . .
COPY --from=web-builder /app/web/dist web/dist
RUN cargo build --release -p serverbee-server

FROM alpine:3.21
RUN apk add --no-cache ca-certificates
COPY --from=rust-builder /app/target/release/serverbee-server /usr/local/bin/
VOLUME /data
ENV SB_SERVER_DATA_DIR=/data
EXPOSE 9527
CMD ["serverbee-server"]
```

```yaml
# docker-compose.yml
services:
  serverbee:
    image: ghcr.io/zingerbee/serverbee:latest
    ports:
      - "9527:9527"
    volumes:
      - serverbee-data:/data
    environment:
      - SB_ADMIN_USERNAME=admin
      - SB_ADMIN_PASSWORD=your_password
    restart: unless-stopped

volumes:
  serverbee-data:
```

### 10.4 安装脚本

```
curl -fsSL https://get.serverbee.io | bash
```

功能:

1. 检测系统架构 (amd64/arm64)
2. 从 GitHub Releases 下载二进制
3. 安装到 `/usr/local/bin/`
4. 创建 `/etc/serverbee/` 配置目录
5. 生成默认配置文件
6. 创建 systemd service unit
7. 启动服务并设置开机自启

**systemd 服务:**

```ini
# /etc/systemd/system/serverbee-server.service
[Unit]
Description=ServerBee Dashboard
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/serverbee-server
WorkingDirectory=/var/lib/serverbee
Restart=always
RestartSec=5
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
```

```ini
# /etc/systemd/system/serverbee-agent.service
[Unit]
Description=ServerBee Agent
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/serverbee-agent
Restart=always
RestartSec=5
AmbientCapabilities=CAP_NET_RAW

[Install]
WantedBy=multi-user.target
```

### 10.5 构建与发布

**CI 构建矩阵:**

| 二进制 | 平台 |
|--------|------|
| `serverbee-server` | linux-amd64, linux-arm64 |
| `serverbee-agent` | linux-amd64, linux-arm64, darwin-amd64, darwin-arm64, windows-amd64 |

**发布流程:**

1. Git tag `v0.1.0`
2. GitHub Actions 触发
3. 构建前端 -> 嵌入 -> 构建各平台二进制
4. 构建 Docker 镜像 -> 推送到 ghcr.io
5. 上传二进制到 GitHub Releases
6. 更新安装脚本版本号

### 10.6 性能预算 (1000 台 Agent)

| 资源 | 预估 |
|------|------|
| Server 内存 | 50-100MB |
| Agent 内存 | 5-15MB |
| SQLite 写入 | ~1000 行/分钟 (WAL 可承载) |
| WebSocket 带宽 | ~160 KB/s |
| 磁盘 (30 天) | ~8 GB |

---

## 11. 功能模块优先级

### P0 — 核心 (MVP)

**Agent:**

- 系统指标采集 (CPU, 内存, Swap, 磁盘, 网络, 负载, 进程数, TCP/UDP, Uptime, OS 信息)
- WebSocket 上报 + 断线重连
- Agent 注册 (auto_discovery_key)

**Server:**

- Agent WebSocket 通信 + 在线/离线检测
- 实时指标缓存 + 浏览器 WebSocket 推送
- 历史指标记录 (1 分钟聚合) + 小时聚合 + 数据清理
- 服务器 CRUD + 分组 + 标签 + 排序
- Session 认证 + API Key 认证
- 系统配置 (K-V)

**前端:**

- 登录页
- 仪表盘总览 (服务器卡片网格, 实时刷新)
- 服务器详情 (指标图表, 时间范围选择)
- 服务器管理 (列表, 分组, 增删改)
- 系统设置
- API Key 管理

### P1 — 重要

**Agent:**

- 温度/GPU 采集
- ICMP/TCP/HTTP 探测
- 远程命令执行
- Web 终端 (PTY)
- GeoIP 上报

**Server:**

- 告警规则引擎 (资源阈值 + 流量周期 + 离线检测)
- 通知系统 (Webhook, Telegram, Email, Bark)
- Ping 任务管理 + 结果存储
- 远程命令下发 + 结果查询
- Web 终端代理
- OAuth (GitHub, Google, OIDC)
- 2FA (TOTP)
- GeoIP 查询

**前端:**

- 告警规则管理页
- 通知渠道管理页
- 探测任务管理页 + 结果图表
- Web 终端页
- OAuth 登录
- 2FA 设置

### P2 — 增强

**Agent:**

- 自动更新
- 远程文件管理
- 虚拟化检测
- 指标采集开关

**Server:**

- 多用户 (Admin/Member 角色)
- DDNS (Cloudflare 等)
- 文件管理器
- 备份恢复
- 审计日志
- WAF (IP 封禁/限流)
- 公开状态页
- Agent 远程升级
- 计费信息管理
- 到期告警

**前端:**

- 用户管理页
- 审计日志页
- 公开状态页
- 文件管理器页
- DDNS 管理页

