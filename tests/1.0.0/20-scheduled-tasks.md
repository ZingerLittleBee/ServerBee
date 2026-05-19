# 20 定时 / 远程任务 — 冒烟测试

**前置条件**:已登录 admin,Agent 启用 CAP_EXEC。深度用例见 [../scheduled-tasks.md](../scheduled-tasks.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| ST1 | 创建一次性任务 | API 新建 oneshot `echo` 任务 | 任务创建,oneshot 自动派发,result 记录返回(agent 因无 EXEC 能力回 "Capability denied: exec disabled on server" exit -2,派发链路验证完整) | 是 | ✅ |
| ST2 | 创建定时任务 | 创建 scheduled cron `0 */1 * * * *` 任务 | 创建成功,next_run_at 计算正确;手动 POST /run 触发,last_run_at 更新 | 是 | ✅ |
| ST3 | 任务结果历史 | GET /api/tasks/:id/results | 列出每次执行 output/exit_code/attempt/起止时间 | 是 | ✅ |
| ST4 | 失败任务 | 执行返回非零退出码命令 | 标记失败,记录错误输出 | 否 | — |
| ST5 | 停止/删除任务 | PUT enabled=false;DELETE 两任务 | 禁用生效(enabled=false),删除后 list 为空 | 否 | ✅ |
| ST6 | 能力关闭 | 未启用 CAP_EXEC 执行任务 | result output="Capability denied: exec disabled on server",exit_code=-2 | 否 | ✅ |

**备注**:cron 表达式为 **6 段**(含秒),5 段返回 validation error。oneshot 任务创建即自动派发(不能 /run,仅 scheduled 可手动 /run)。测试 agent (0.9.3) agent_local_capabilities=60 不支持 CAP_EXEC(2),服务端开启也无法生效,故所有命令实际执行均被能力拦截 — 但任务创建/调度/派发/结果记录全链路均已验证(ST6 的 denied 结果正是 agent 收到派发的证明)。ST4 因 exec 全部被能力拒绝,无法构造真实非零退出码场景区分,记 —(原因:agent 本地不支持 EXEC,环境限制)。任务已全部清理。

**汇总**:✅ 5 / ❌ 0 / — 1 (ST4 因 agent 本地不支持 EXEC 无法验证)
