# ServerBee

语言: [English](./README.md) | 简体中文

轻量级、自托管的 VPS 监控探针系统，基于 Rust 和 React 构建。

## 功能特性

- **实时仪表盘** -- 服务器状态、CPU/内存/磁盘/网络指标，WebSocket 实时推送
- **服务器分组** -- 按组管理服务器，显示国旗标识
- **详细指标** -- 实时流式图表 + 历史视图 (1h/6h/24h/7d/30d)，涵盖 CPU、内存、磁盘、网络、负载、温度、GPU
- **告警系统** -- 14+ 指标类型，阈值/离线/流量/到期规则，AND 逻辑，70% 采样
- **通知渠道** -- Webhook、Telegram、Bark、Email (SMTP)，支持通知组
- **Ping 探测** -- ICMP、TCP、HTTP 探测，延迟图表和成功率统计
- **Web 终端** -- 基于 WebSocket 代理的浏览器 PTY 终端
- **GPU 监控** -- NVIDIA GPU 使用率/温度/显存 (nvml-wrapper，可选功能)
- **GeoIP** -- 根据 Agent IP 自动检测地区/国家 (MaxMind MMDB)
- **OAuth & 2FA** -- GitHub/Google/OIDC 登录，TOTP 两步验证
- **多用户** -- Admin/Member 角色，审计日志，速率限制
- **能力开关** -- 每台服务器独立的功能控制 (终端、执行、升级、探测)，服务端+Agent 双重校验
- **公共状态页** -- 无需登录的服务器状态展示
- **计费追踪** -- 价格、计费周期、到期提醒、流量限制
- **备份恢复** -- SQLite 数据库备份/恢复 API
- **Agent 自动更新** -- 远程二进制升级，SHA-256 校验
- **OpenAPI 文档** -- Swagger UI (`/swagger-ui`)，50+ 完整注释端点

## 技术栈

| 组件 | 技术 |
|------|------|
| 服务端 | Rust, Axum 0.8, sea-orm, SQLite (WAL) |
| Agent | Rust, sysinfo 0.33, tokio-tungstenite |
| 前端 | React 19, Vite 7, TanStack Router/Query, Recharts, shadcn/ui, Tailwind CSS v4 |
| 认证 | argon2 密码哈希, Session Cookie, API Key, OAuth2, TOTP |
| 文档 | Fumadocs MDX, TanStack Start, 中英双语 |

## 快速开始

### 前置条件

- Rust 1.85+ (含 cargo)
- Bun 1.x (用于前端构建)

### 从源码构建

```bash
# 克隆
git clone https://github.com/ZingerLittleBee/ServerBee.git
cd ServerBee

# 构建前端
cd apps/web && bun install && bun run build && cd ../..

# 构建服务端和 Agent
cargo build --release

# 二进制文件位于:
# target/release/serverbee-server
# target/release/serverbee-agent
```

### 启动服务端

```bash
./serverbee-server
# 默认地址: http://localhost:9527
# 管理员密码在启动日志中自动生成并打印
# Auto-discovery key 也会在首次启动时打印
```

### 启动 Agent

```bash
# 通过环境变量设置服务端地址和发现密钥
SERVERBEE_SERVER_URL=http://your-server:9527 \
SERVERBEE_AUTO_DISCOVERY_KEY=YOUR_KEY \
./serverbee-agent

# 或创建配置文件 /etc/serverbee/agent.toml:
# server_url = "http://your-server:9527"
# auto_discovery_key = "YOUR_KEY"
```

注册成功后，Agent 会将 token 保存到配置文件，重启后自动重连。

### Docker

```bash
docker compose up -d
```

### 开发模式 (Make)

```bash
# 同时启动服务端 (端口 9527) + Vite 开发服务器 (端口 5173)
make dev-full
# 访问 http://localhost:5173，使用 admin / admin123 登录

# 或分步启动:
make server-dev                                           # 终端 1: 服务端 :9527
SERVERBEE_AUTO_DISCOVERY_KEY="<key>" make agent-dev       # 终端 2: Agent

# 测试与代码质量:
make cargo-test        # 运行全部 Rust 测试 (121)
make test              # 运行前端测试 (72)
make cargo-clippy      # Rust 代码检查
make                   # 交互式菜单 (需要 fzf)
```

服务端启动时会打印完整的 auto-discovery key，复制后启动 Agent。

> **说明**: `make dev-full` 启动带 HMR 的 Vite 开发服务器 (`http://localhost:5173`)，自动代理 `/api/*` 到 Rust 服务端 (`:9527`)。生产构建请使用 `make build` 然后 `make server-run`。

## 配置

所有配置项均可通过 TOML 文件或环境变量设置，环境变量使用 `SERVERBEE_` 前缀，`__` (双下划线) 作为嵌套分隔符。完整环境变量列表见 [ENV.md](ENV.md)。

### 服务端 (`/etc/serverbee/server.toml`)

```toml
[server]
listen = "0.0.0.0:9527"
data_dir = "/var/lib/serverbee"

[database]
path = "serverbee.db"
max_connections = 10

[auth]
session_ttl = 86400           # 24 小时
secure_cookie = true          # 开发环境设为 false
auto_discovery_key = ""       # 留空自动生成

[admin]
username = "admin"
password = ""                 # 留空自动生成

[rate_limit]
login_max = 5                 # 15 分钟内最大登录尝试次数
register_max = 3              # 15 分钟内最大 Agent 注册次数

[retention]
records_days = 7              # 原始指标保留天数
records_hourly_days = 90      # 小时聚合保留天数
audit_logs_days = 180         # 审计日志保留天数

[geoip]
enabled = false
mmdb_path = "/var/lib/serverbee/GeoLite2-City.mmdb"
```

环境变量示例:
```bash
export SERVERBEE_ADMIN__PASSWORD="my-secure-password"
export SERVERBEE_GEOIP__ENABLED=true
export SERVERBEE_OAUTH__GITHUB__CLIENT_ID="..."
```

### Agent (`/etc/serverbee/agent.toml`)

```toml
server_url = "http://your-server:9527"
token = ""                    # 注册后自动填充
auto_discovery_key = ""       # 仅用于首次注册

[collector]
interval = 3                  # 指标上报间隔 (秒)
enable_temperature = true
enable_gpu = false            # 需要 NVIDIA GPU + nvml

[log]
level = "info"
```

Agent 环境变量使用 `SERVERBEE_` 前缀，顶层键无需嵌套:
```bash
export SERVERBEE_SERVER_URL="http://your-server:9527"
export SERVERBEE_AUTO_DISCOVERY_KEY="YOUR_KEY"
```

### OAuth 配置

```toml
[oauth]
base_url = "https://monitor.example.com"
allow_registration = false    # 首次 OAuth 登录时自动创建用户

[oauth.github]
client_id = "..."
client_secret = "..."

[oauth.google]
client_id = "..."
client_secret = "..."
```

回调 URL 格式: `https://your-domain/api/auth/oauth/{provider}/callback`

## 部署

### Systemd

```bash
# 安装服务端
sudo bash deploy/install.sh server

# 安装 Agent
sudo bash deploy/install.sh agent
```

Service 文件位于 `deploy/` 目录:
- `serverbee-server.service`
- `serverbee-agent.service`

### 反向代理 (Nginx)

```nginx
server {
    listen 443 ssl;
    server_name monitor.example.com;

    location / {
        proxy_pass http://127.0.0.1:9527;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # WebSocket (浏览器 + Agent + 终端)
    location /api/ws/ {
        proxy_pass http://127.0.0.1:9527;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_read_timeout 86400s;
        proxy_send_timeout 86400s;
    }

    location /api/agent/ws {
        proxy_pass http://127.0.0.1:9527;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_read_timeout 86400s;
        proxy_send_timeout 86400s;
    }
}
```

## API

服务端运行时可通过 `/swagger-ui` 访问交互式 API 文档。

## 许可证

[AGPL-3.0](LICENSE)
