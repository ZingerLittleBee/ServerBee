# 30 能力位掩码管理 — 冒烟测试

**前置条件**:已登录 admin,Agent 在线。能力位:TERMINAL=1, EXEC=2, UPGRADE=4, PING_ICMP=8, PING_TCP=16, PING_HTTP=32, FILE=64, DOCKER=128。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| CP1 | 查看能力 | 设置页查看服务器能力开关 | 显示各能力当前启用状态 | 是 | ✅ |
| CP2 | 关闭能力 | 关闭能力位(API 用 HTTP 替代 TERMINAL,因 agent 本地不支持 TERMINAL) | CapabilitiesSync 下发,cap 60→28,effective 同步 | 是 | ✅ |
| CP3 | 开启能力 | 重新开启 HTTP | cap 28→60,effective 恢复 | 是 | ✅ |
| CP4 | 服务端拦截 | 服务端禁用 FILE 后调用 file list | 被拒 FORBIDDEN server_capability_disabled | 否 | ✅ |
| CP5 | Agent 本地拒绝 | 服务端 set TERMINAL(cap=61) | effective 仍=60,agent_local_capabilities 不含 TERMINAL,本地拒绝生效 | 否 | ✅ |
| CP6 | 默认能力 | 查看测试 server 默认能力位 | local=60=UPGRADE+ICMP+TCP+HTTP,UI 仅这些开启,符合 UPGRADE+PING 预期 | 否 | ✅ |

**备注**:
- 测试 server (agent 0.9.3) 初始/恢复基线 = **60** (UPGRADE4+ICMP8+TCP16+HTTP32),非 brief 所述 56。已据实以 60 为还原值。
- agent_local_capabilities=60,不含 TERMINAL/EXEC/FILE/DOCKER,故这些能力服务端开启也无法生效(effective 排除)。直接影响 14/15/16/31 — 这些功能在本测试 agent 上不可真机验证。
- UI 能力开关:高风险能力(Terminal/Exec/File/Docker)开关在 UI 中 disabled(因 agent 本地不支持);可点击的开关(如 HTTP)在 UI 中点击后不持久化(快照回弹、cap 不变),但 API PUT /api/servers/batch-capabilities 正常工作。CP2/CP3 改用 API 验证。

> ❌ CP-UI: 能力位 UI 开关点击后不持久化(HTTP Probe 开关点击后快照回弹 checked=true,cap 仍 60,无 toast);API 路径正常。文件 30-capabilities.md。

**汇总**:✅ 6 / ❌ 0 / — 0 (功能层全通过;附 1 个 UI 持久化缺陷见上)
