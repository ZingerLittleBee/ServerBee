# 06 Agent enrollment 注册 — 冒烟测试

**前置条件**:Server 已启动,admin 已登录。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| E1 | 铸造 enrollment code | `POST /api/agent/enrollments` 或设置页生成 | 返回一次性 code + 过期时间(默认 10 分钟) | 是 | ✅ |
| E2 | Agent 注册 | 用 code 启动 Agent(`SERVERBEE_ENROLLMENT_CODE`) | Agent 注册成功,获得 server_id + token 并持久化 | 是 | ✅ |
| E3 | Agent 上线 | 注册后观察 `/servers` | 新服务器出现,状态 Online,指标流入 | 是 | ✅ |
| E4 | code 单次使用 | 用同一 code 再次注册 | 被拒(已使用) | 是 | ✅ |
| E5 | code 过期 | 过期后用旧 code 注册 | 被拒(已过期) | 否 | — |
| E6 | token 轮换 | 触发 token 轮换 | 旧 token 失效,Agent 用新 token 重连成功 | 否 | — |
| E7 | 注册管理 | 设置页查看 enrollment 列表 | 显示状态(未用/已用/过期),可撤销 | 否 | ✅ |

> ✅ E1: `POST /api/agent/enrollments`→code + `expires_at`(铸造后约 10 分钟)。E2/E3: Agent 持 code 注册成功,`/api/servers` 出现 server `a90af3c2…`(macOS, agent 0.9.3),server 日志 `Agent ... connected from 127.0.0.1` 确认 WS 在线。E4: 同 code 再次 `POST /api/agent/register`→401(单次使用)。E7: `GET /api/agent/enrollments` 返回 `code_prefix`/`consumed_at`/`expires_at` 状态列表。
> — E5: 未等待 10 分钟过期窗口,本轮跳过。— E6: 未触发 token 轮换,本轮跳过(深度用例见 `../agent-enrollment-smoke.md`)。

**汇总**:✅ 5 / ❌ 0 / — 2
