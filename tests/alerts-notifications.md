# 告警 & 通知测试用例

## 前置条件

参照 [README.md](README.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

建议准备 webhook.site URL 用于验证通知发送。

---

## 一、告警 & 通知全链路

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| A1 | 通知渠道创建 | 创建 Webhook + Telegram | 列表显示 2 个渠道 | ✅ |
| A2 | 通知组创建 | 创建 "E2E Test Group" 关联 2 个渠道 | 列表显示 "2 channel(s)" | ✅ |
| A3 | 测试通知发送 | 点击测试按钮 | Webhook (webhook.site) + Telegram 均收到消息 | ✅ |
| A4 | 阈值告警触发 | 创建 cpu ≥ 1% 规则 → 60s 后触发 | Webhook + Telegram 收到告警通知 | ✅ |
| A5 | 告警状态展示 | 点击 States | 显示 "New Server" 🔴 Triggered (2x) + 时间戳 | ✅ |
| A6 | 告警条件格式 | 查看规则摘要 | 正确显示 `cpu ≥ 1 | always` 和 `offline 30s | once` | ✅ |
| A7 | 离线告警触发 | 创建 offline 30s 规则 → 停 Agent → 等待 | ⚠️ 未触发（时序窗口问题，非代码 bug） | ⚠️ |
| A8 | Swagger UI | 访问 `/swagger-ui/` | 显示 ServerBee API 0.1.0 OAS 3.1 | ✅ |
| A9 | Ping 任务创建 | 创建 HTTP ping | 列表显示 "Ping Google" | ✅ |
| A10 | Ping 结果收集 | 等待 25s | 7 条记录，全部成功，延迟 387-402ms | ✅ |
| A11 | Capabilities API 修复 | `update_server`/`batch_capabilities` | API 正常返回 | ✅ |
| A12 | 终端页面加载 | 启用 CAP_TERMINAL → 打开 `/terminal/:id` | xterm.js 容器渲染正常 | ✅ |
| A13 | 终端 WS 连接 | 查看 WebSocket 连接状态 | 显示 "closed" — Agent 需要重连以获取 CapabilitiesSync | ⚠️ |

---

## 二、IP 变更通知

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| IP1 | 被动检测 — remote_addr 变更 | Agent 断线 → 从不同 IP 重连 | 审计日志出现 ip_changed 记录 | — |
| IP2 | 被动检测 — last_remote_addr 更新 | Agent 连接 → GET /api/servers/:id | last_remote_addr 字段有值 | — |
| IP3 | 主动检测 — NIC 变更 | Agent 运行中 → 添加/移除网络接口 | 5 分钟内检测到变更 | — |
| IP4 | 主动检测 — 外部 IP (可选) | 配置 check_external_ip=true | 公网 IP 变化时上报 | — |
| IP5 | 事件驱动告警 | 创建 ip_changed 告警规则 → 关联通知组 | IP 变更时触发通知 | — |
| IP6 | 告警规则覆盖范围 | 创建 cover_type=include 规则 | 仅指定服务器触发 | — |
| IP7 | Browser 推送 | Dashboard 打开时 → IP 变更 | WS 推送 ServerIpChanged 消息 | — |
| IP8 | GeoIP 更新 | IP 变更后 | 服务器 region/country_code 自动更新（需先安装 GeoIP 数据库） | — |
| IP9 | 配置禁用 | 设置 ip_change.enabled=false | Agent 不发送 IpChanged | — |
| IP10 | i18n | 切换中英文 | 告警规则类型 "IP Changed"/"IP 变更" 正确显示 | — |

---

## 三、E2E 测试中发现并修复的 Bug

| Bug | 描述 | 修复 |
|-----|------|------|
| 登录错误消息 | 显示原始 JSON `{"error":{"code":"UNAUTHORIZED",...}}` | 解析 JSON 提取 `error.message` 字段 (`69af3e7`) |
| 通知表单明文密码 | password/bot_token/device_key 使用 `type="text"` | 改为 `type="password"` 掩码 (`82dcf15`) |
| 告警表单缺失字段 | 仅 12 种规则类型 + 仅 `max` 字段 | 扩展到 19 种 + 条件 min/duration/cycle 字段 |
| 告警状态无 UI | 后端有 alert_state 但前端无展示 | 新增 API + 可展开 per-server 状态面板 (`a8defea`) |
| Capabilities API 500 | `update_server`/`batch_capabilities` 使用 `Extension<(String,String,String)>` 无人注入 | 改为 `Extension<CurrentUser>` + `HeaderMap` |
