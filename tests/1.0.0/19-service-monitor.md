# 19 服务监控(HTTP/TCP/DNS/SSL/Whois) — 冒烟测试

**前置条件**:已登录 admin。深度用例见 [../service-monitor.md](../service-monitor.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| M1 | HTTP 监控 | `/settings/service-monitors` 新建 HTTP 监控 | 周期检查,状态 Up/Down | 是 | ✅ |
| M2 | TCP 监控 | 新建 TCP 端口监控 | 连通性检查正常 | 是 | ✅ |
| M3 | DNS 监控 | 新建 DNS 解析监控 | 解析结果检查 | 否 | ✅ |
| M4 | SSL 证书 | 新建 SSL 监控 | 显示证书到期天数 | 否 | ❌ |
| M5 | Whois | 新建 Whois 监控 | 显示域名到期信息 | 否 | ✅ |
| M6 | 详情历史 | 访问 `/service-monitors/:id` | 历史可用性图表 | 是 | ✅ |
| M7 | 手动触发检查 | 点击立即检查 | 立刻返回最新状态 | 否 | ✅ |
| M8 | 状态变更告警 | 目标宕机 | 触发关联通知 | 否 | ✅ |

> ❌ M4: SSL 监控检查请求挂起后被丢弃,服务端无响应(curl exit 52 / HTTP 000),不写入任何记录、last_checked 永远为 "Never"。复现:建 monitor_type=ssl(target `example.com:443` 或 `example.com`),POST `/api/service-monitors/{id}/check` 或 UI 点 "Trigger check";其它类型 HTTP/TCP/DNS/Whois 同环境正常,服务端不崩溃,仅 SSL 检查路径异常。阻断级=否。

**汇总**:✅ 7 / ❌ 1 / — 0
