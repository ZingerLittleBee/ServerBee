# 30 能力（Agent 拥有）— 冒烟测试

**前置条件**:已登录 admin,Agent 在线。能力位:TERMINAL=1, EXEC=2, UPGRADE=4, PING_ICMP=8, PING_TCP=16, PING_HTTP=32, FILE=64, DOCKER=128, SECURITY_EVENTS=256, FIREWALL_BLOCK=512, IP_QUALITY=1024。`CAP_DEFAULT=1852`。

**模型**:能力由 Agent 主机拥有。Agent 从 `[capabilities]` 配置(allow/deny over `CAP_DEFAULT`)+ `--allow-cap`/`--deny-cap` CLI 计算自身能力并在 `SystemInfo` 上报。Server 只把上报值镜像到 `servers.capabilities` 用于展示,**无法**修改。`effective == agent_local`。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| CP1 | 查看能力(详情) | 服务器详情 → 点击「能力」 | 只读弹窗,展示各能力 已启用/已关闭 徽章,无开关;含「能力在 Agent 配置文件中设置」提示 | 是 | ⬜ |
| CP2 | 查看能力(机群) | 设置 → 能力开关 | 只读矩阵(✓/—),无开关、无批量操作栏、无勾选框;描述说明只读/Agent 拥有 | 是 | ⬜ |
| CP3 | Server 无法改能力 | `PUT /api/servers/{id}` 带 `{"capabilities": N}` | `capabilities` 字段被忽略(不在 DTO 中),server 能力镜像不变 | 是 | ⬜ |
| CP4 | 批量端点已移除 | `PUT /api/servers/batch-capabilities` | 404 Not Found(端点已删除) | 否 | ⬜ |
| CP5 | 服务端按上报能力拦截 | Agent 未上报 FILE 时调用 file list | 403 FORBIDDEN,reason=`agent_capability_disabled` | 否 | ⬜ |
| CP6 | Agent 配置生效 | 改 agent `[capabilities] allow=["terminal"]` 并重启 agent | 重连后详情/矩阵中 TERMINAL 显示 已启用;`CapabilitiesChanged` 推送刷新 UI | 否 | ⬜ |
| CP7 | 默认能力 | 查看一台无 `[capabilities]` 覆盖的 agent | 能力=`1852`(UPGRADE+ICMP+TCP+HTTP+SECURITY_EVENTS+FIREWALL_BLOCK+IP_QUALITY),高风险 Terminal/Exec/File/Docker 关闭 | 否 | ⬜ |

**备注**:
- 旧的服务端能力开关、`CapabilitiesSync` 下发、`PUT /api/servers/batch-capabilities` 批量端点均已移除;协议版本升至 5。
- 能力来源唯一为 Agent 主机配置;如需改某 agent 能力,编辑该主机 `[capabilities]` 并重启 agent,不能从 Server/UI 修改。
- 离线服务器仍可从镜像列展示最后已知能力。
