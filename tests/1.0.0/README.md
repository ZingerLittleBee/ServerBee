# ServerBee 1.0.0 发布冒烟测试

发布前对全部已实现功能做一次整体冒烟验证。冒烟测试只覆盖每个功能点的**核心 happy-path**,确认功能可用、无明显回归,不追求穷尽边界用例(深度用例见 `tests/` 根目录对应文件)。

## 环境准备

启动本地环境(Server + Agent + 登录 + enrollment code)的步骤参见上级 [../README.md](../README.md)。

建议覆盖矩阵:
- 浏览器:Chrome(桌面) + 移动端尺寸(< 768px)
- 主题:亮色 / 暗色
- 语言:中文 / 英文
- 角色:admin / member(只读校验)
- 部署:本地 cargo + Docker Compose 各跑一遍关键流程

## 状态图例

`☐` 待测 · `✅` 通过 · `❌` 失败 · `—` 本轮不适用/缺环境

## 测试索引

| # | 文件 | 功能点 |
|---|------|--------|
| 01 | [01-auth-login.md](01-auth-login.md) | 登录 / 登出 / 会话 |
| 02 | [02-onboarding.md](02-onboarding.md) | 首次登录引导(强制改密) |
| 03 | [03-security.md](03-security.md) | 安全设置(密码 / 2FA / OAuth) |
| 04 | [04-user-management.md](04-user-management.md) | 用户管理与 RBAC |
| 05 | [05-api-keys.md](05-api-keys.md) | API 密钥 |
| 06 | [06-agent-enrollment.md](06-agent-enrollment.md) | Agent enrollment 注册 |
| 07 | [07-dashboard.md](07-dashboard.md) | 自定义仪表盘 |
| 08 | [08-server-list.md](08-server-list.md) | 服务器列表 / 分组 / 标签 |
| 09 | [09-server-detail.md](09-server-detail.md) | 服务器详情与指标图表 |
| 10 | [10-disk-io.md](10-disk-io.md) | 磁盘 I/O 监控 |
| 11 | [11-gpu-monitoring.md](11-gpu-monitoring.md) | GPU 监控 |
| 12 | [12-traffic.md](12-traffic.md) | 流量统计与账单周期 |
| 13 | [13-cost.md](13-cost.md) | 成本洞察 |
| 14 | [14-docker.md](14-docker.md) | Docker 容器监控与操作 |
| 15 | [15-file-manager.md](15-file-manager.md) | 文件管理 |
| 16 | [16-terminal.md](16-terminal.md) | Web 终端 |
| 17 | [17-ping-tasks.md](17-ping-tasks.md) | Ping 探测任务 |
| 18 | [18-network-quality.md](18-network-quality.md) | 网络质量监控 / Traceroute |
| 19 | [19-service-monitor.md](19-service-monitor.md) | 服务监控(HTTP/TCP/DNS/SSL/Whois) |
| 20 | [20-scheduled-tasks.md](20-scheduled-tasks.md) | 定时 / 远程任务 |
| 21 | [21-alerts.md](21-alerts.md) | 告警规则 |
| 22 | [22-notifications.md](22-notifications.md) | 通知渠道 |
| 23 | [23-uptime.md](23-uptime.md) | Uptime 时间线 |
| 24 | [24-status-page.md](24-status-page.md) | 公开状态页 |
| 25 | [25-incident-maintenance.md](25-incident-maintenance.md) | 事件管理与维护窗口 |
| 26 | [26-geoip.md](26-geoip.md) | GeoIP 数据库 |
| 27 | [27-audit-logs.md](27-audit-logs.md) | 审计日志 |
| 28 | [28-appearance-theme.md](28-appearance-theme.md) | 主题 / 外观 / 品牌 |
| 29 | [29-i18n.md](29-i18n.md) | 国际化 |
| 30 | [30-capabilities.md](30-capabilities.md) | 能力位掩码管理 |
| 31 | [31-agent-upgrade.md](31-agent-upgrade.md) | Agent 自动升级 |
| 32 | [32-mobile.md](32-mobile.md) | Mobile API 与 iOS App |
| 33 | [33-server-recovery.md](33-server-recovery.md) | 服务器恢复任务 |
| 34 | [34-backup-restore.md](34-backup-restore.md) | 备份与还原 |
| 35 | [35-deployment.md](35-deployment.md) | 部署与安装(脚本 / Docker / systemd) |
| 36 | [36-realtime-websocket.md](36-realtime-websocket.md) | 实时 WebSocket 推送 |
| 37 | [37-responsive.md](37-responsive.md) | 响应式与移动端布局 |

## 发布判定

- 所有 `01`–`37` 文件中标记为「阻断级」的用例必须 ✅ 才能发布 1.0.0。
- 非阻断级 ❌ 需在发布说明中登记为已知问题。

## 汇总

| 指标 | 值 |
|------|-----|
| 功能点总数 | 37 |
| ✅ 通过 | |
| ❌ 失败 | |
| — 不适用 | |
| 通过率 | |
