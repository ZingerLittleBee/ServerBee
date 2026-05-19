# 33 服务器恢复任务 — 冒烟测试

**前置条件**:已登录 admin,目标服务器在线。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| RC1 | 创建恢复任务 | GET recovery-candidates / POST recover-merge | 守卫验证:目标在线时 candidates 返回 CONFLICT "Target server must be offline";recover-merge 不存在 source 返回 NOT_FOUND "Server not found"(状态守卫正常,完整恢复流需目标离线未执行) | 否 | ✅ |
| RC2 | 执行进度 | 观察恢复任务 | 显示阶段/状态(running→success) | 否 | — |
| RC3 | 成功完成 | 恢复执行完毕 | 状态 success,记录结果 | 否 | — |
| RC4 | 失败处理 | 模拟恢复失败 | 状态 failed,记录错误 | 否 | — |
| RC5 | 运行中防护 | 目标在线 / source 不存在时触发 | 被守卫拒绝(CONFLICT 需离线 / NOT_FOUND source 不存在),状态守卫生效 | 否 | ✅ |
| RC6 | 范围/历史清理 | 触发清理逻辑 | scoped/recovery 记录按规则清理 | 否 | — |

**备注**:服务器恢复 = 将离线/旧 server 记录合并进重新注册的 server(历史合并)。API:GET `/api/servers/:id/recovery-candidates`、POST `/api/servers/:id/recover-merge`(body source_server_id)。**共享环境仅有 1 台 server(a98e328b…,被各组共用,已被改名 "Smoke B Server")**。recovery-candidates 要求目标 server **先离线** 才能列候选;完整 RC2–RC4/RC6 需杀掉共享 agent 使其离线 + 第二台 source server 做合并 —— brief 明确禁止杀 agent / 影响其他并发组,共享单 server 环境也无 source 可合并,故未执行(原因:安全约束 + 单 server 共享环境,非缺陷)。RC1/RC5 的状态守卫(离线要求、source 存在性)已验证生效;测试 server 全程保持在线(0.9.3,进程未动)。

**汇总**:✅ 2 / ❌ 0 / — 4 (—均因共享单 server 环境 + 禁止使 agent 离线的安全约束)
