# Agent Authority 竞态与加固测试

这组用例验证 Agent enrollment 的安全边界。Server 身份由管理员 onboarding 创建，不从 Agent 主机信息推导；Agent Authority 只负责 offer、run token、状态转换、连接 fencing 和事件历史。

## 一、自动化覆盖

| 测试组 | 主要覆盖 |
|--------|----------|
| `service::server_onboarding` | Server、标签、默认探测、初始 offer、事件和幂等记录同事务提交；失败完整回滚 |
| `service::agent_authority` | 单 outstanding 约束、四种终态、claim 竞态、严格 offer CAS、graceful/emergency、删除后事件留存 |
| `agent_registration_integration` | 真实 HTTP/WS claim、fencing、幂等 replay、状态与历史投影 |
| Agent `register` / `reporter` | claim 前持久化 run token、模糊 HTTP 结果后的 WS-first 恢复、401 处理、日志不泄密 |

运行：

```bash
cargo test -p serverbee-server service::server_onboarding
cargo test -p serverbee-server service::agent_authority
cargo test -p serverbee-server --test agent_registration_integration
cargo test -p serverbee-agent register
cargo test -p serverbee-agent reporter
```

## 二、Onboarding 幂等与原子性

| # | 操作 | 预期 |
|---|------|------|
| RH-1 | 使用相同 actor、`onboarding_request_id` 和等价输入连续或并发调用 `POST /api/servers` | 只有一个 Server、一条 onboarding 记录和一个 outstanding offer；所有响应指向同一 `server_id` |
| RH-2 | 同一 request ID 搭配不同标准化输入 | 返回 409 `ONBOARDING_IDEMPOTENCY_CONFLICT`，不产生额外状态 |
| RH-3 | 制造标签、默认探测或 offer 创建失败 | Server、标签、默认探测、offer、事件与幂等记录全部回滚 |
| RH-4 | 重放已成功 onboarding | `replayed=true`，绝不返回明文 code；只返回当前 outstanding offer 元数据（如有） |

## 三、Claim 与 offer 竞态

| # | 操作 | 预期 |
|---|------|------|
| RH-5 | 两个 Agent 使用同一 code 和不同 proposed run token 并发 claim | 只有一个成功；offer 只进入一次 `consumed`；胜者 token 成为唯一有效 authority |
| RH-6 | 两个管理员同时替换同一准确 offer ID | 只有一个成功；另一个得到 409 stale/terminal；数据库仍最多一个 outstanding offer |
| RH-7 | 用旧页面中的 offer ID 替换或吊销 | 不能影响较新的 outstanding offer |
| RH-8 | 让 offer 超时后再 claim/replace | claim 返回 401，管理操作返回终态冲突，历史只记录 `expired` 一次 |

## 四、Authority 和 WebSocket fencing

| # | 操作 | 预期 |
|---|------|------|
| RH-9 | claimed 状态开始 graceful re-enrollment | 旧 token 和当前连接继续有效，直到新 claim 原子替换 authority |
| RH-10 | claimed 状态开始 emergency re-enrollment | 旧 authority 立即失效、旧连接关闭，同时产生一个 offer |
| RH-11 | 独立 `DELETE /api/servers/{id}/agent-authority` | authority 变为 unclaimed、连接被关闭，不隐式产生 offer |
| RH-12 | 在 WS preflight 与 upgrade/final admission 间吊销 authority | final admission 拒绝旧 token；fencing 后任何旧连接 frame 都不能进入业务分发 |

## 五、Cleanup 与安装持久化

| # | 操作 | 预期 |
|---|------|------|
| RH-13 | 创建一个从未 claim 的离线 Server 和一个已连线但未发 `SystemInfo` 的 Agent，再调用 `DELETE /api/servers/cleanup` | 只删除离线未初始化 Server，在线连接对应记录保留；各自删除事件正确写入 |
| RH-14 | 用 Docker 安装 Agent 并重启容器 | Agent 配置目录持久化，claim 前暂存的 run token 不丢失；重启直接使用 token，不重复消费 offer |

## 六、UI/API 投影

| # | 操作 | 预期 |
|---|------|------|
| RH-15 | 查看 REST 列表、Browser WS、Web 和 iOS | `agent_authority.status` 与 `outstanding_offer` 一致；online/offline 不改变 claimed/unclaimed |
| RH-16 | 触发每类转换后读取 `/api/agent-authority/events` | actor、source、mode、offer outcome、before/after 完整，不含明文 code/run token；Server 删除后仍可读取 |
