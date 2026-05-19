# 23 Uptime 时间线 — 冒烟测试

**前置条件**:已登录,Agent 有在线/离线历史。深度用例见 [../uptime.md](../uptime.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| UP1 | 详情页时间线 | 服务器详情查看 uptime | 90 天时间线色块(正常/降级/宕机/无数据) | 是 | ✅ |
| UP2 | 宕机记录 | 停止 Agent 一段时间后恢复 | 对应时段标记 Down,恢复后变正常 | 是 | — |
| UP3 | Dashboard widget | 添加 uptime-timeline widget | 渲染时间线 + 图例 | 否 | ✅ |
| UP4 | 状态页时间线 | 公开状态页 `/status/:slug` | 显示组件 uptime 时间线 | 否 | ❌ |
| UP5 | 可用率统计 | 查看可用率百分比 | 数值与时间线一致 | 否 | ✅ |

> ❌ UP4: 公开状态页 /status/smoke-e 仅显示分组(Smoke Group B)下服务器的实时 CPU/Memory/Disk/网络/uptime 数值卡片,未渲染 90 天 uptime 时间线色块与图例(Operational/Down)。复现:打开 /status/smoke-e → 仅见实时指标卡片,无时间线组件。注:可能受状态页配置项控制(该状态页由其它组用默认设置创建),但默认状态页未展示时间线,与 UP4 预期"显示组件 uptime 时间线"不符。
> UP2 —: 共享测试 Agent 不可停止(子代理约束),无法主动制造宕机时段;离线转换在文件 36 WS3 覆盖。
> 备注: UP1 详情页 "Uptime (90 days)" 时间线含 Operational/Degraded/Down/No data 图例,可用率 0.30%(仅约 2 天历史/90 天,符合)。UP3 dashboard uptime-timeline widget 正确渲染服务器名+0.30%+90天轴+图例(默认尺寸偏小但结构完整)。UP5 可用率 0.30% 与时间线大面积"无数据"一致。

**汇总**:✅ 3 / ❌ 1 / — 1(UP2 受共享环境约束未执行)
