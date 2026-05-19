# 25 事件管理与维护窗口 — 冒烟测试

**前置条件**:已登录 admin,已有一个公开状态页。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| IM1 | 创建事件(incident) | 新建事件,设标题/影响/状态 | 事件创建并显示在状态页 | 否 | ✅ |
| IM2 | 事件更新 | 追加 update(调查中→已解决) | 时间线追加更新条目 | 否 | ✅ |
| IM3 | 关闭事件 | 标记事件已解决 | 状态页恢复正常态 | 否 | ✅ |
| IM4 | 创建维护窗口 | 设定维护时间段 | 状态页显示计划维护提示 | 否 | ✅ |
| IM5 | 维护期间 | 进入维护时间段 | 组件标记维护中,不误报宕机 | 否 | ✅ |
| IM6 | 删除事件/维护 | 删除条目 | 状态页移除对应展示 | 否 | ✅ |

> IM1/IM2: 事件创建后进入 `active_incidents`,追加 update 后 status 推进且 `updates` 时间线追加。
> IM3: resolved 后 `active_incidents=0`、`resolved_at` 写入,`recent_incidents` 保留历史(页面恢复正常)。
> IM4/IM5: 维护窗口进入 `planned_maintenances`,维护期内组件 `in_maintenance=true` 且 `online=true`(标记维护中不误报宕机)。
> IM6: 删除事件/维护后 `recent_incidents=0`/`planned_maintenances=0`/`in_maintenance=false`。

**汇总**:✅ 6 / ❌ 0 / — 0
