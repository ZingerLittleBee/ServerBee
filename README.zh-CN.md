<div align="center">

<img src="assets/logo/logo.svg" width="96" alt="ServerBee logo" />

# ServerBee

**轻量、自托管的 VPS 监控系统 —— 一个 Rust 二进制,实时掌控一切。**

[![CI](https://github.com/ZingerLittleBee/ServerBee/actions/workflows/ci.yml/badge.svg)](https://github.com/ZingerLittleBee/ServerBee/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/ZingerLittleBee/ServerBee?include_prereleases&sort=semver)](https://github.com/ZingerLittleBee/ServerBee/releases)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)
[![GitHub stars](https://img.shields.io/github/stars/ZingerLittleBee/ServerBee?style=flat)](https://github.com/ZingerLittleBee/ServerBee/stargazers)
[![Rust](https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black)](https://react.dev)

[English](./README.md) | 简体中文

</div>

---

ServerBee 让你在一处掌控所有服务器。中心 **Server** 通过 WebSocket 接收来自轻量 **Agent** 的指标,存入内嵌 SQLite,并提供实时 React 仪表盘 —— 无外部数据库,无沉重运行时。

- 🪶 **极致轻量** —— Agent 通常仅占用约 5–15 MB 内存,Server 即便管理大量节点也保持精简。
- ⚡ **实时刷新** —— WebSocket 实时仪表盘,涵盖 CPU、内存、磁盘、网络、负载、温度、GPU、磁盘 I/O。
- 📦 **单一二进制** —— Server 与内嵌 Web UI 打包成一个文件,支持 Docker、一行脚本、Railway 部署。
- 🔋 **开箱即用** —— 告警、通知、Web 终端、文件管理、Docker、防火墙、状态页等一应俱全。
- 🔒 **默认安全** —— OAuth + 2FA、RBAC、审计日志、一次性 Agent 注册、逐服务器能力门控。

> [!NOTE]
> ServerBee 正在活跃开发中(`v1.0.0-alpha`),迭代频繁。

## 快速开始

### 1. 安装 Server

```bash
curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- server --method docker
```

打开 `http://your-server:9527`。管理员密码会自动生成并打印在启动日志中 —— 首次登录后请修改。

> 安装脚本通过 `--method docker|binary` 同时支持 **Docker** 与 **二进制** 两种安装方式。**Server 推荐 Docker 安装**;省略该参数则进入交互式选择。偏好云端?使用下方的 [Railway 一键部署](#railway一键部署)。

### 2. 接入 Agent

以管理员登录 → **设置** → 生成一个一次性 **enrollment code**(单次使用,约 10 分钟后过期)。然后在每个节点上:

```bash
curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- agent --method binary \
  --server-url http://YOUR_SERVER:9527 --enrollment-code YOUR_ONE_TIME_CODE
```

> **Agent 推荐二进制安装** —— 占用最小,且能采集完整的宿主机指标。如需在容器中运行 Agent,改用 `--method docker`。

Agent 首次连接时会保存每服务器 token 并自动重连 —— code 只需用一次。搞定。🎉

## 功能特性

| | |
|---|---|
| **📊 监控** | 实时指标(CPU/内存/磁盘/网络/负载/温度/GPU/磁盘 I/O)· 历史图表(1h–30d)· Docker 容器统计、日志与事件 · 按计费周期统计月度流量并预测 |
| **🔔 告警** | 14+ 指标类型 · 阈值 / 离线 / 流量 / 到期规则 · Webhook、Telegram、Bark、邮件渠道,支持通知组 |
| **🌐 网络** | Ping 探测(ICMP/TCP/HTTP)· 网络质量监控(96 个中国三网 + 国际预设)· 服务监控(SSL/WHOIS/HTTP/Ping/TCP)· IP 质量与流媒体解锁检测,含欺诈风险评分 |
| **🛠️ 远程管理** | 浏览器 Web 终端(WS 上的 PTY)· 沙箱化文件管理 + Monaco 编辑器 · 基于 nftables 的防火墙封禁 · 逐服务器能力开关 · Agent 自动更新 |
| **🔐 安全与访问** | SSH 登录 / 暴力破解 / 端口扫描检测 · OAuth(GitHub/Google/OIDC)+ TOTP 两步验证 · Admin/Member 角色 · 审计日志 · 一次性 Agent 注册码 |
| **🖥️ 仪表盘与分享** | 拖拽式自定义仪表盘(13 种 widget)· 含 90 天可用性时间线的公共状态页 · OKLCH 自定义主题 · 带国旗的服务器分组 · 原生 iOS 移动端 |
| **⚙️ 运维** | `serverbee` 管理 CLI · 备份与恢复 · GeoIP 地区检测 · OpenAPI/Swagger 文档(50+ 端点) |

## 配置

通过 TOML 文件或 `SERVERBEE_` 前缀的环境变量配置(`__` 为嵌套分隔符,如 `SERVERBEE_AUTH__MAX_SERVERS`)。最小可运行配置:

```toml
# /etc/serverbee/server.toml
[server]
listen = "0.0.0.0:9527"
data_dir = "/var/lib/serverbee"

[admin]
password = ""   # 留空自动生成
```

```toml
# /etc/serverbee/agent.toml
server_url = "http://your-server:9527"
enrollment_code = ""   # 来自设置页的一次性 code,仅用于首次注册

[collector]
interval = 3           # 上报间隔(秒)
```

📖 完整参考:**[ENV.md](ENV.md)** · OAuth、数据保留、速率限制、GeoIP 等详见[文档](apps/docs)。

## 部署

### Railway(一键部署)

[![Deploy on Railway](https://railway.com/button.svg)](https://railway.com/deploy/serverbee-server)

添加挂载到 `/data` 的 Volume 以持久化数据。Server 首次启动会自动创建管理员账号 —— 在部署日志中查找凭据横幅。

### 管理 CLI

安装脚本会在 `/usr/local/bin/serverbee` 放置一个 `serverbee` CLI:

```bash
sudo serverbee status         # 查看所有组件状态
sudo serverbee upgrade -y     # 升级到最新版
sudo serverbee restart        # 重启服务
sudo serverbee config         # 查看 / 修改配置
sudo serverbee uninstall agent -y
```

### 反向代理

在 Nginx/Caddy 之后,将 `/` 代理到 `127.0.0.1:9527`,并确保 WebSocket 路由 `/api/ws/` 和 `/api/agent/ws` 透传 `Upgrade`/`Connection` 头且设置较长读超时。完整可用的 Nginx 配置见[部署文档](apps/docs)。

## 开发

```bash
git clone https://github.com/ZingerLittleBee/ServerBee.git
cd ServerBee

make dev-full         # Server(:9527)+ Vite 开发服务器(:5173)—— 使用 admin / admin123 登录
make cargo-test       # Rust 测试
make test             # 前端测试
make cargo-clippy     # Rust 代码检查
```

> `make dev-full` 启动带 HMR 的 Vite(`http://localhost:5173`),并代理 `/api/*` 到 `:9527` 的 Rust 服务端。在 **设置** 页生成一次性 enrollment code 即可接入开发用 Agent。

**技术栈:** Rust(Axum 0.8 · sea-orm · SQLite WAL)· React 19(Vite 7 · TanStack Router/Query · Recharts · shadcn/ui · Tailwind CSS v4)· Rust Agent(sysinfo · tokio-tungstenite)。

## API

服务端运行时,可在 `/swagger-ui` 访问交互式 OpenAPI 文档。

## 许可证

[AGPL-3.0](LICENSE)
