# 23 Uptime 时间线 — 冒烟测试

**前置条件**:已登录,Agent 有在线/离线历史。深度用例见 [../uptime.md](../uptime.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| UP1 | 详情页时间线 | 服务器详情查看 uptime | 90 天时间线色块(正常/降级/宕机/无数据) | 是 | ✅ |
| UP2 | 宕机记录 | 停止 Agent 一段时间后恢复 | 对应时段标记 Down,恢复后变正常 | 是 | — |
| UP3 | Dashboard widget | 添加 uptime-timeline widget | 渲染时间线 + 图例 | 否 | ✅ |
| UP4 | 状态页时间线 | 公开状态页 `/status/:slug` | 显示组件 uptime 时间线 | 否 | ✅ |
| UP5 | 可用率统计 | 查看可用率百分比 | 数值与时间线一致 | 否 | ✅ |

> ✅ UP4(**原整功能不可用,已修复**): `/status/:slug` 自定义状态页(90 天 `UptimeTimeline`、事件、维护、品牌/自定义 CSS)整页不可达的路由缺陷已修复。根因:`status.tsx`(`/status`,组件 `StatusPage`)未渲染 `<Outlet/>`,而 `status.$slug.tsx` 是其子路由,TanStack 子路由需父级 Outlet 才挂载 → slug 页永不渲染,且连带遮蔽 24-status-page / 25 公开事件维护。修复:将 `status.tsx` 拆为布局(仅渲染 `<Outlet/>`),聚合页迁至新 `status.index.tsx`(路由 `/status/`),TanStack 路由树已重新生成(`StatusIndexRoute` + `StatusSlugRoute` 均为 `/status` 子路由)。前端 typecheck/lint 干净、499 测试全绿。**编排者真机端到端复验(重建前端后,无登录访问)**:建自定义状态页 `/status/fv-final`(绑定 server + 一个 incident「FV Incident Open/major/investigating」+ 一个活跃维护窗口 + 自定义 CSS),无登录访问截图确认渲染**自定义页**——标题/品牌、Partial System Outage 横幅、Active Incidents 区、Planned Maintenance 区、Server Status 行含 **90 天 UptimeTimeline 色块条** + 可用率,自定义 CSS 注入;`/status`(无 slug)仍正常渲染聚合页(CPU/Memory/Disk,无 404)。连带验证 25:incident `POST /updates`→identified、`PUT`→resolved 均 200;24:status-pages/maintenances 列表 200。**修复确认生效。**
> UP2 —: 共享测试 Agent 不可停止(子代理约束),无法主动制造宕机时段;离线转换在文件 36 WS3 覆盖。
> 备注: UP1 详情页 "Uptime (90 days)" 时间线含 Operational/Degraded/Down/No data 图例,可用率 0.30%(仅约 2 天历史/90 天,符合)。UP3 dashboard uptime-timeline widget 正确渲染服务器名+0.30%+90天轴+图例(默认尺寸偏小但结构完整)。UP5 可用率 0.30% 与时间线大面积"无数据"一致。

**汇总**:✅ 4 / ❌ 0 / — 1(UP4 状态页路由已修复,真机端到端复验通过;UP2 受共享环境约束未执行)
