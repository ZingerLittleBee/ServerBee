# 22 通知渠道 — 冒烟测试

**前置条件**:已登录 admin,进入 `/settings/notifications`。深度用例见 [../alerts-notifications.md](../alerts-notifications.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| NT1 | 配置 Email | 填写 SMTP 配置 → 测试发送 | 收到测试邮件 | 是 | ☐ |
| NT2 | 配置 Telegram | 填写 bot token/chat id → 测试 | 收到 Telegram 消息 | 否 | ☐ |
| NT3 | 配置 Webhook | 填写 URL → 测试 | 目标收到 webhook 请求 | 是 | ☐ |
| NT4 | 配置 Bark | 填写 Bark key → 测试 | 收到 Bark 推送 | 否 | ☐ |
| NT5 | APNS 推送 | 配置 APNS(移动端) | iOS 设备收到推送 | 否 | ☐ |
| NT6 | 通知组 | 创建通知组并关联渠道 | 告警按组分发 | 否 | ☐ |
| NT7 | 删除渠道 | 删除某渠道 | 移除,关联告警不再用该渠道 | 否 | ☐ |

**汇总**:✅ ___ / ❌ ___ / — ___
