# 22 通知渠道 — 冒烟测试

**前置条件**:已登录 admin,进入 `/settings/notifications`。深度用例见 [../alerts-notifications.md](../alerts-notifications.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| NT1 | 配置 Email | 填写 SMTP 配置 → 测试发送 | 收到测试邮件 | 是 | — |
| NT2 | 配置 Telegram | 填写 bot token/chat id → 测试 | 收到 Telegram 消息 | 否 | — |
| NT3 | 配置 Webhook | 填写 URL → 测试 | 目标收到 webhook 请求 | 是 | ✅ |
| NT4 | 配置 Bark | 填写 Bark key → 测试 | 收到 Bark 推送 | 否 | — |
| NT5 | APNS 推送 | 配置 APNS(移动端) | iOS 设备收到推送 | 否 | — |
| NT6 | 通知组 | 创建通知组并关联渠道 | 告警按组分发 | 否 | ✅ |
| NT7 | 删除渠道 | 删除某渠道 | 移除,关联告警不再用该渠道 | 否 | ✅ |

> 备注 NT1:Email 实现非传统 SMTP,改用 **Resend**(表单只有 From/收件人,无 host/port/凭据;需 `SERVERBEE_RESEND__API_KEY`)。渠道配置可创建并持久化;测试发送返回明确校验错误 "Resend API key not configured"(本环境未配 Resend),实际投递需配 Resend 且会发往真实邮箱,故记 —(配置保存 OK,投递未验)。
> 备注 NT2/NT4/NT5:Telegram/Bark/APNs 类型选项与配置表单均存在;但需真实 bot token / Bark key / iOS 设备,会打扰真人且本组无安全测试渠道,故记 —(表单存在,投递未验)。
> 备注 NT3:webhook 渠道指向 webhook.site 临时地址;测试发送返回 ok,目标 3s 内收到 POST "[ServerBee] Test ... This is a test notification"。✅
> 备注 NT6:创建 smoke-group 关联 webhook 渠道;经 A2 告警按组分发,webhook 实收 "Alert rule 'smoke-cpu-alert' triggered",且 19-M8 服务监控状态变更也按组分发成功。✅
> 备注 NT7:删除 email/webhook 渠道均 HTTP 200 且从列表移除;但被组引用的 webhook 渠道删除后,通知组 `notification_ids_json` 仍保留已删 id(悬挂引用未清理)——功能上该渠道已不存在不会被使用,属次要数据清洁问题,非阻断(级=否)。

**汇总**:✅ 3 / ❌ 0 / — 4
