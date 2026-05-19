# 18 网络质量监控 / Traceroute — 冒烟测试

**前置条件**:已登录,已配置网络探针目标。深度用例见 [../network-quality.md](../network-quality.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| N1 | 探针配置 | `/settings/network-probes` 添加目标 SmokeProbe(8.8.8.8) | 目标保存,出现在 Target 表 | 是 | ✅ |
| N2 | 总览页 | 访问 `/network` | 显示 Network Quality Overview(Total Servers/Avg Latency/Availability) | 是 | ✅ |
| N3 | 详情页 | 访问 `/network/:serverId` | 页面结构正常(Avg Latency/Availability 区块);新建探针暂无数据显示 No data(时序待 agent 采集,符合预期) | 是 | ✅ |
| N4 | Traceroute | POST /api/servers/:id/traceroute target=8.8.8.8 | 返回 request_id,轮询得 2 跳路由(rtt 完整)completed=true | 否 | ✅ |
| N5 | 异常检测 | 制造网络抖动/丢包 | 标记网络异常 | 否 | — |
| N6 | 删除探针 | DELETE /api/network-probes/targets/:id | 移除,list 中 SmokeProbe 消失 | 否 | ✅ |

**备注**:网络探针 API = `/api/network-probes/targets`(GET/POST/DELETE)。N5 需在共享环境真实制造网络抖动/丢包,违反"勿改环境"约束未执行(原因:环境约束,非缺陷)。N6 UI 下拉操作菜单在当前视口(表格 Manage 列溢出视口右侧 + Radix portal 菜单 snapshot 难捕获)未能经 UI 完成,改用 API 验证删除成功 — 附注 UI 可用性观察见下。

> ❌ N6-UI: 网络探针 Target 表格在 ~1280px 视口下宽度溢出,Manage 操作列(More actions 按钮)被推到可视区右侧外,无法在该视口经 UI 删除目标(API 删除正常)。文件 18-network-quality.md。

**汇总**:✅ 4 / ❌ 0 / — 1 (N5 环境约束;附 1 个 UI 响应式/操作可达性观察见上)
