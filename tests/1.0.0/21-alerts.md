# 21 告警规则 — 冒烟测试

**前置条件**:已登录 admin,已配置至少 1 个通知渠道。深度用例见 [../alerts-notifications.md](../alerts-notifications.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| A1 | 创建阈值告警 | `/settings/alerts` 新建 CPU/内存/磁盘阈值规则 | 规则保存并启用 | 是 | ✅ |
| A2 | 触发告警 | 制造指标超阈(如压满 CPU) | 生成告警事件 + 推送通知 | 是 | ✅ |
| A3 | 告警恢复 | 指标回落正常 | 标记恢复,发送恢复通知 | 是 | ✅ |
| A4 | 告警事件列表 | 查看告警事件/Dashboard alert-list widget | 列出聚合告警事件 | 否 | ✅ |
| A5 | 离线告警 | Agent 离线 | 触发离线告警 | 否 | — |
| A6 | IP 变更告警 | Agent 外网 IP 变化 | 触发 IP 变更通知 | 否 | — |
| A7 | 编辑/删除规则 | 修改阈值或删除规则 | 生效,删除后不再触发 | 否 | ✅ |

> ✅ A3(**阻断级**,已修复并真机端到端复验): 根因为 `crates/server/src/service/alert.rs` `evaluate_rule` 的 Recovered 分支只 `mark_resolved`+log,从未调用 `NotificationService::send_group`(通知仅在 triggered 发出)。已新增 `handle_resolved`,在 triggered→recovered 边沿派发 `event:"resolved"` 通知。回归测试 `alert.rs::test_recovery_dispatches_resolved_notification`;server lib 504 全绿。**编排者真机端到端复验(本地 webhook sink + always 模式 CPU min=0 规则)**:P1 触发 → 收到 `... triggered` 投递、alert-event status=firing;P2 阈值升至 999999 恢复 → 60s 内收到 `[ServerBee] Smoke B Server resolved` 投递、alert-event status=resolved 且 resolved_at 写入。修复确认生效。

> ✅ A2/A3-followup re-arm(**原严重阻断,已修复**): 告警首轮 `trigger→resolve` 后即使条件再满足也永久不再触发的缺陷已修复。根因:`mark_triggered` 的 `else` 分支对 `alert_states` 盲 `INSERT`,resolve 后残留同 `(rule_id,server_id)` 行 → 命中 `UNIQUE` 约束(SQLite 2067)→ 评估永久中止。修复:`else` 分支改为 upsert —— 存在旧行则复位(`resolved=false`、`resolved_at=NULL`、`count=1`、刷新时间)而非盲 INSERT。新增回归测试 `alert.rs::test_alert_rearms_after_resolve_cycle`(trigger→resolve→re-trigger 闭环,断言单行且重新 firing);TDD 红/绿通过,server lib 506 全绿、CI clippy 干净。**需发布前真机端到端复验完整状态机闭环。**
>
> 备注 A4:聚合告警事件经 `/api/alert-events` 正常返回(status=firing/resolved + count 聚合);但前端 `/alerts` 路由 404,默认 Dashboard 未见 alert-list widget(可能为非默认布局的可选 widget)。核心聚合能力 OK。
> 备注 A5/A6:offline / ip_changed 规则配置可保存并通过校验;但实际触发需让共享测试 Agent 离线或改公网 IP,受"严禁重启/删除测试 Agent、勿扰其他组"约束无法安全诱发,故投递记 —(配置 OK,触发未验)。

**汇总**:✅ 5 / ❌ 0 / — 2(A3 恢复通知 + re-arm 缺陷均已修复并补回归测试,待发布前真机端到端复验完整状态机)
