# 17 Ping 探测任务 — 冒烟测试

**前置条件**:已登录 admin,Agent 启用 PING 能力位。深度用例见 [../ping-tasks.md](../ping-tasks.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| P1 | 创建 ICMP 任务 | API 新建 ICMP 8.8.8.8 任务 | 任务创建 (id 返回),Agent 开始探测 | 是 | ✅ |
| P2 | 创建 TCP 任务 | 新建 TCP 1.1.1.1:443 探测 | 创建成功,record success latency 0.85ms | 是 | ✅ |
| P3 | 创建 HTTP 任务 | 新建 HTTP example.com 探测 | 创建成功,record success latency 145ms | 是 | ✅ |
| P4 | 结果与图表 | 查询 task records (需 from/to 参数) | 三类探测均返回 success record (ICMP 302ms/TCP 0.85ms/HTTP 145ms) | 是 | ✅ |
| P5 | 编辑/停止任务 | PUT 改 ICMP interval 60→30、HTTP enabled→false | 修改生效,字段同步 | 否 | ✅ |
| P6 | 删除任务 | DELETE 三个任务 | 全部删除,list 为空 | 否 | ✅ |
| P7 | 能力被拒 | unset PING_ICMP(8),观察 ICMP 探测 | cap→52,移除后 45s+ 无新 ICMP record(探测被能力拦截停止);恢复后 cap→60 | 否 | ✅ |

**备注**:经 API 验证(UI 创建入口存在 Add 按钮)。records 接口需 `from`/`to` query 参数(缺失返回 missing field `from`)。能力位测试后已还原 cap=60,任务全部清理。

**汇总**:✅ 7 / ❌ 0 / — 0
