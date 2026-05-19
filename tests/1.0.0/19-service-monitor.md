# 19 服务监控(HTTP/TCP/DNS/SSL/Whois) — 冒烟测试

**前置条件**:已登录 admin。深度用例见 [../service-monitor.md](../service-monitor.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| M1 | HTTP 监控 | `/settings/service-monitors` 新建 HTTP 监控 | 周期检查,状态 Up/Down | 是 | ✅ |
| M2 | TCP 监控 | 新建 TCP 端口监控 | 连通性检查正常 | 是 | ✅ |
| M3 | DNS 监控 | 新建 DNS 解析监控 | 解析结果检查 | 否 | ✅ |
| M4 | SSL 证书 | 新建 SSL 监控 | 显示证书到期天数 | 否 | ✅ |
| M5 | Whois | 新建 Whois 监控 | 显示域名到期信息 | 否 | ✅ |
| M6 | 详情历史 | 访问 `/service-monitors/:id` | 历史可用性图表 | 是 | ✅ |
| M7 | 手动触发检查 | 点击立即检查 | 立刻返回最新状态 | 否 | ✅ |
| M8 | 状态变更告警 | 目标宕机 | 触发关联通知 | 否 | ✅ |

> ✅ M4(**原整功能不可用,已修复**): SSL 监控每次检查 panic tokio worker 的缺陷已修复。根因:`crates/server/src/service/checker/ssl.rs` 用 rustls 0.23 `ClientConfig::builder()`,而 aws_lc_rs 与 ring 两个 crypto feature 同时被编译进(reqwest rustls-tls + rustls 默认),无法自动选定 provider → `rustls-0.23.37/src/crypto/mod.rs:249` panic。修复:改用 `ClientConfig::builder_with_provider(ring::default_provider())` 显式 provider(ring 经 reqwest 必然可用),并对 provider 配置失败返回失败 CheckResult 而非 panic。新增回归测试 `ssl.rs::test_ssl_check_builds_tls_config_without_panicking`(不可达主机应返回失败而非 panic);TDD 红(panic)/绿通过,server lib 506 全绿。**编排者真机端到端复验(重建二进制后)**:新建 SSL 监控 `example.com:443` → `POST /api/service-monitors/{id}/check` 返回 200(226ms,非 3ms 断连/非超时),`success:true`、`days_remaining:43`、issuer=Cloudflare TLS Issuing ECC CA 1、not_after=Wed 01 Jul 2026、subject=CN=example.com、sha256 指纹齐全;server 日志 0 条 rustls/CryptoProvider/panic。**修复确认生效。**

**汇总**:✅ 8 / ❌ 0 / — 0(M4 SSL panic 已修复,真机对真实 HTTPS 站点复验通过)
