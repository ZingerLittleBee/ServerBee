# E2E 手动验证测试

## 启动本地环境

```bash
# 1. 构建前端（server 通过 rust-embed 嵌入 dist/）
cd apps/web && bun install && bun run build && cd ../..

# 2. 构建 Rust
cargo build --workspace

# 3. 启动 Server（设置管理员密码，开发环境关闭 secure cookie）
SERVERBEE_ADMIN__PASSWORD=admin123 SERVERBEE_AUTH__SECURE_COOKIE=false cargo run -p serverbee-server &

# 4. 获取 auto-discovery key（登录后调用 API）
curl -s -c /tmp/sb-cookies.txt -X POST http://localhost:9527/api/auth/login \
  -H 'Content-Type: application/json' -d '{"username":"admin","password":"admin123"}'
curl -s -b /tmp/sb-cookies.txt http://localhost:9527/api/settings/auto-discovery-key
# 返回 {"data":{"key":"<discovery_key>"}}

# 5. 启动 Agent（server_url 是 HTTP 基础地址，不是 WS 路径）
SERVERBEE_SERVER_URL="http://127.0.0.1:9527" SERVERBEE_AUTO_DISCOVERY_KEY="<discovery_key>" cargo run -p serverbee-agent &

# Docker 方式
docker compose up -d
```

默认地址：`http://localhost:9527`，管理员用户名：`admin`

> **注意**：`SERVERBEE_SERVER_URL` 应设置为 HTTP 基础地址（如 `http://127.0.0.1:9527`），Agent 会自动拼接 `/api/agent/register` 和 `/api/agent/ws?token=` 路径。

## 测试文件索引

| 文件 | 功能 | 路由 |
|------|------|------|
| [auth-users.md](auth-users.md) | 认证、用户与安全 | `/login`, `/settings/users`, `/settings/api-keys` |
| [dashboard.md](dashboard.md) | 自定义仪表盘 | `/` |
| [server-detail.md](server-detail.md) | 服务器列表与详情 | `/servers`, `/servers/:id` |
| [ping-tasks.md](ping-tasks.md) | Ping 探测任务管理 | `/settings/ping-tasks` |
| [network-quality.md](network-quality.md) | 网络质量监控 | `/network`, `/network/:id`, `/settings/network-probes` |
| [docker.md](docker.md) | Docker 容器监控 | `/servers/:id/docker` |
| [disk-io.md](disk-io.md) | 磁盘 I/O 监控 | `/servers/:id` (历史模式) |
| [traffic.md](traffic.md) | 月度流量统计 | `/traffic`, `/servers/:id` (Traffic tab) |
| [file-manager.md](file-manager.md) | 文件管理 | `/servers/:id` (Files) |
| [service-monitor.md](service-monitor.md) | 服务监控 | `/settings/service-monitors`, `/service-monitors/:id` |
| [scheduled-tasks.md](scheduled-tasks.md) | 定时任务 | `/settings/tasks` (Scheduled tab) |
| [security.md](security.md) | 安全设置（密码、2FA、OAuth） | `/settings/security` |
| [alerts-notifications.md](alerts-notifications.md) | 告警 & 通知 + IP 变更 | `/settings/alerts`, `/settings/notifications` |
| [uptime.md](uptime.md) | Uptime 90 天时间线 | `/status/:slug`, `/servers/:id`, Dashboard widget |
| [general-settings.md](general-settings.md) | 通用设置（Key、备份） | `/settings` |
| [geoip.md](geoip.md) | GeoIP 数据库管理 | `/settings/geoip` |
| [status-page.md](status-page.md) | 状态页增强 | `/status/:slug`, `/settings/status-pages` |
| [appearance.md](appearance.md) | 主题、品牌、响应式 | `/settings/appearance` |
| [audit-logs.md](audit-logs.md) | 审计日志 | `/settings/audit-logs` |
| [i18n.md](i18n.md) | 国际化 | 全站 |
| [terminal.md](terminal.md) | Web 终端 | `/terminal/:serverId` |
| [performance.md](performance.md) | 前端性能测试 | `/servers/:id` (realtime) |
| [mobile-ios.md](mobile-ios.md) | iOS 移动端 & Mobile API | `/api/mobile/*`, `/settings/mobile-devices`, iOS App |

## 页面渲染快速验证

| 功能 | 路由 | 状态 |
|------|------|------|
| 登录 | `/login` | ✅ |
| Dashboard | `/` | ✅ |
| Servers 列表 | `/servers` | ✅ |
| 服务器详情 | `/servers/:id` | ✅ |
| 网络质量总览 | `/network` | ✅ |
| 网络质量详情 | `/network/:id` | ✅ |
| Docker 监控 | `/servers/:id/docker` | — |
| 流量总览 | `/traffic` | ✅ |
| 流量 Traffic Tab | `/servers/:id` (Traffic tab) | ✅ |
| 服务监控列表 | `/settings/service-monitors` | ✅ |
| 服务监控详情 | `/service-monitors/:id` | ✅ |
| 用户管理 | `/settings/users` | ✅ |
| 通知 | `/settings/notifications` | ✅ |
| 告警 | `/settings/alerts` | ✅ |
| API Keys | `/settings/api-keys` | ✅ |
| Security | `/settings/security` | ✅ |
| 审计日志 | `/settings/audit-logs` | ✅ |
| 远程命令 | `/settings/tasks` | ✅ |
| 公共状态页 | `/status` | ✅ |
| Swagger UI | `/swagger-ui/` | ✅ |
| 终端 | `/terminal/:id` | ✅ |
| 移动设备管理 | `/settings/mobile-devices` | — |
