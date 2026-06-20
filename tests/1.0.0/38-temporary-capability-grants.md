# 38 临时能力授予（主机本地） — 冒烟测试

**前置条件**:已登录 admin;一台**可被你完全控制**的测试 Agent 主机(可 grant/revoke/重启/SSH),且其 `agent.toml` 中某个高危能力(`terminal`/`exec`/`file`/`docker`)**默认关闭**(即不在 `[capabilities] allow` 中)。能确认 `serverbee-agent` 二进制在该主机 PATH 上,且对 `state_dir`(默认 `/var/lib/serverbee`)有写权限(通常需 `sudo`)。

> ⚠️ 注意:本用例需重启测试 Agent,**请勿在共享测试 Agent 上执行**,使用专用可控主机(见 `reference_vps_reverse_tunnel_agent.md`)。

**模型**:能力由 Agent 主机拥有。`serverbee-agent grant <cap> --for <30m|2h|1d> [--reason "..."]` 在主机本地把一个默认关闭的能力临时开启,写入 `<state_dir>/capability_grants.json`(绝对 `expires_at`,`0600`)。守护进程数秒内拾取并重新上报有效能力;Server 实时打开门控、镜像变更、审计、(高危则)触发 `capability_grant_detected` 告警。Server/UI 只读,无法授予。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| TG1 | grant 高危能力 | 主机上 `sudo serverbee-agent grant terminal --for 30m --reason "smoke"` | 命令打印 `Granted 'terminal' for 30m (expires_at epoch ...)`,退出码 0;写入 `capability_grants.json` | 是 | ⬜ |
| TG2 | UI 出现 Temporary 徽章(详情) | 服务器详情 → 能力弹窗 | TERMINAL 显示为已启用,带琥珀色 **Temporary** 徽章 + 实时倒计时(约 30m 递减) | 是 | ⬜ |
| TG3 | UI 出现 Temporary 徽章(矩阵) | 设置 → 能力开关 | 该 server 的 TERMINAL 列显示已启用 + Temporary 徽章/倒计时 | 是 | ⬜ |
| TG4 | 临时能力可用 | 在授予窗口内,从 Web 打开该 server 的终端(或对应高危功能) | 功能可正常使用(WS 升级不再 403),纵深防御不再拒绝 | 是 | ⬜ |
| TG5 | grant 审计 | 设置 → 审计日志 | 出现 `capability_temporarily_granted`,detail 含 cap/expires_at/granted_by/reason | 否 | ⬜ |
| TG6 | grant 告警(可选) | 预先配置 `capability_grant_detected` 告警规则 + 通知组,再执行 TG1 | grant 时收到一次通知(高危能力) | 否 | ⬜ |
| TG7 | grants 列表 | 主机上 `serverbee-agent grants` | 列出 `terminal`、剩余秒数、granted_by、reason | 否 | ⬜ |
| TG8 | 跨重启存活 | 授予窗口中途重启 Agent(`systemctl restart serverbee-agent`) | 重连后能力仍为已启用,Temporary 徽章/倒计时按**原始**截止时间继续(重启不延长) | 是 | ⬜ |
| TG9 | 过期自动关闭 | 等待到期(或先 `grant terminal --for 60s` 复现,等其过期) | 到期后能力变回关闭(徽章消失);再用该功能被拒绝(`agent_capability_disabled`) | 是 | ⬜ |
| TG10 | 过期审计 | 过期后查看审计日志 | 出现 `capability_grant_expired`(detail 含 cap) | 否 | ⬜ |
| TG11 | 提前撤销 | 新 grant 后 `sudo serverbee-agent revoke terminal` | 命令打印 `Revoked temporary grant for 'terminal'.`;UI 徽章消失、能力关闭;审计出现 `capability_grant_revoked` | 否 | ⬜ |
| TG12 | 拒绝已开启能力 | 对 `agent.toml` 中已 allow 的能力执行 grant | 报错 `'<cap>' is already enabled in agent.toml; nothing to grant`,退出码 1 | 否 | ⬜ |
| TG13 | 拒绝超长时长 | `serverbee-agent grant terminal --for 2d`(默认 max 24h) | 报错 `duration '2d' exceeds temporary_max_duration ('24h'); refusing` | 否 | ⬜ |
| TG14 | 失败安全(corrupt) | 手动把 `capability_grants.json` 写成无效 JSON,等守护进程下一轮读取 | 视作无授予,能力保持关闭(不 panic、不崩溃) | 否 | ⬜ |
| TG15 | 断连授予不告警 | 先停 Server(或断 Agent 网络)→ 主机 grant → 再恢复连接 | 重连后 UI 显示 Temporary 徽章(能力生效),但**无** `capability_temporarily_granted` 审计/告警(Server 只见重连后状态,看不到转换) | 否 | ⬜ |
| TG16 | Server/UI 无法授予 | 确认 Web/iOS 能力页只读,无任何「授予/临时开启」按钮 | UI 无授予入口;能力只能由主机 CLI 改 | 否 | ⬜ |

**备注**:
- 临时授予只能把**默认关闭**的位翻为开启,永远不能增强永久能力集;过期/撤销/低风险授予只审计、不告警。
- `capability_grant_detected` 为事件驱动告警,仅高危能力(terminal/exec/file/docker)触发;不带 `severity_min`/`exclude_cidrs`/自动封禁。
- 时长单位:`s`/`m`/`h`/`d`(秒/分/时/天);`expires_at` 为绝对 Unix epoch,故跨重启不变。
- 文档参考:`apps/docs/content/docs/{en,zh}/capabilities.mdx`(临时授予段)、`alerts.mdx`、`admin.mdx`。
